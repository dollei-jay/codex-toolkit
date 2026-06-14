use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::{
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};
use tungstenite::{connect, stream::MaybeTlsStream, Message, WebSocket};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;
const DEFAULT_DEBUG_PORT: u16 = 9229;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockRequest {
    #[serde(default = "default_debug_port")]
    pub debug_port: u16,
    #[serde(default)]
    pub restart_codex: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockResult {
    pub debug_port: u16,
    pub restarted: bool,
    pub injected: bool,
    pub target_title: Option<String>,
    pub target_url: Option<String>,
    pub app_path: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CdpTarget {
    #[serde(rename = "type")]
    target_type: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: Option<String>,
}

pub fn unlock_plugins_default(request: UnlockRequest) -> Result<UnlockResult, String> {
    let debug_port = if request.debug_port == 0 {
        DEFAULT_DEBUG_PORT
    } else {
        request.debug_port
    };
    let mut restarted = false;
    let mut app_path = codex_process_path();

    if let Ok(target) = retry_pick_target(debug_port) {
        let websocket_url = target
            .web_socket_debugger_url
            .clone()
            .ok_or_else(|| "Selected Codex target has no websocket URL.".to_string())?;
        evaluate_script(&websocket_url, PLUGIN_UNLOCK_SCRIPT)?;
        return Ok(UnlockResult {
            debug_port,
            restarted,
            injected: true,
            target_title: Some(target.title),
            target_url: Some(target.url),
            app_path,
            message: "Plugin unlock script injected into the running Codex window.".to_string(),
        });
    }

    if request.restart_codex {
        if app_path.is_some() {
            return Err(format!(
                "Codex is already running without DevTools on port {debug_port}. Close Codex first, then launch it from this toolkit."
            ));
        }
        let launch = find_codex_launch_target()
            .ok_or_else(|| "Could not find Codex app install path.".to_string())?;
        app_path = Some(launch.display_path().to_string());
        start_codex_launch_target(&launch, debug_port)?;
        restarted = true;
    }

    let target = retry_pick_target(debug_port)?;
    let websocket_url = target
        .web_socket_debugger_url
        .clone()
        .ok_or_else(|| "Selected Codex target has no websocket URL.".to_string())?;
    evaluate_script(&websocket_url, PLUGIN_UNLOCK_SCRIPT)?;

    Ok(UnlockResult {
        debug_port,
        restarted,
        injected: true,
        target_title: Some(target.title),
        target_url: Some(target.url),
        app_path,
        message: "Plugin unlock script injected. Open the Codex plugin page to verify.".to_string(),
    })
}

fn default_debug_port() -> u16 {
    DEFAULT_DEBUG_PORT
}

fn retry_pick_target(debug_port: u16) -> Result<CdpTarget, String> {
    let mut last_error = None;
    for _ in 0..24 {
        match list_targets(debug_port).and_then(|targets| pick_page_target(&targets)) {
            Ok(target) => return Ok(target),
            Err(error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(500));
            }
        }
    }
    Err(last_error.unwrap_or_else(|| {
        format!("Could not connect to Codex DevTools on port {debug_port}. Restart Codex with debug mode first.")
    }))
}

fn list_targets(debug_port: u16) -> Result<Vec<CdpTarget>, String> {
    let response = http_get_localhost(debug_port, "/json")?;
    serde_json::from_str(&response).map_err(|error| format!("Could not parse CDP targets: {error}"))
}

fn pick_page_target(targets: &[CdpTarget]) -> Result<CdpTarget, String> {
    let mut first_page = None;
    for target in targets {
        if target.target_type != "page"
            || target
                .web_socket_debugger_url
                .as_deref()
                .unwrap_or("")
                .is_empty()
        {
            continue;
        }
        first_page.get_or_insert(target);
        let haystack = format!("{} {}", target.title, target.url).to_lowercase();
        if haystack.contains("codex") {
            return Ok(target.clone());
        }
    }
    first_page
        .cloned()
        .ok_or_else(|| "No injectable Codex page target found.".to_string())
}

fn evaluate_script(websocket_url: &str, script: &str) -> Result<Value, String> {
    let (mut socket, _) = connect(websocket_url).map_err(|error| error.to_string())?;
    let command = json!({
        "id": 1,
        "method": "Runtime.evaluate",
        "params": {
            "expression": script,
            "awaitPromise": false,
            "allowUnsafeEvalBlockedByCSP": true
        }
    });
    socket
        .send(Message::Text(command.to_string()))
        .map_err(|error| error.to_string())?;
    read_cdp_response(&mut socket, 1)
}

fn read_cdp_response(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    id: u64,
) -> Result<Value, String> {
    for _ in 0..40 {
        let message = socket.read().map_err(|error| error.to_string())?;
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        if value.get("id").and_then(Value::as_u64) != Some(id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            return Err(format!("CDP evaluate failed: {error}"));
        }
        return Ok(value);
    }
    Err("CDP evaluate timed out.".to_string())
}

fn http_get_localhost(port: u16, path: &str) -> Result<String, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|error| format!("Could not connect to 127.0.0.1:{port}: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| error.to_string())?;
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|error| error.to_string())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| error.to_string())?;
    let (_, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "Invalid HTTP response from CDP endpoint.".to_string())?;
    Ok(body.to_string())
}

#[cfg(target_os = "windows")]
fn codex_process_path() -> Option<String> {
    powershell_output(
        "Get-Process | Where-Object { $_.ProcessName -eq 'Codex' -or $_.ProcessName -eq 'OpenAI Codex' } | Select-Object -First 1 -ExpandProperty Path",
    )
    .ok()
    .map(|output| output.trim().to_string())
    .filter(|output| !output.is_empty())
}

#[cfg(not(target_os = "windows"))]
fn codex_process_path() -> Option<String> {
    None
}

#[derive(Clone, Debug)]
enum CodexLaunchTarget {
    Executable {
        path: String,
        display_path: String,
    },
    #[cfg(target_os = "windows")]
    Packaged {
        app_user_model_id: String,
        display_path: String,
    },
    #[cfg(target_os = "macos")]
    MacosApp {
        app_dir: String,
        display_path: String,
    },
}

impl CodexLaunchTarget {
    fn display_path(&self) -> &str {
        match self {
            Self::Executable { display_path, .. } => display_path,
            #[cfg(target_os = "windows")]
            Self::Packaged { display_path, .. } => display_path,
            #[cfg(target_os = "macos")]
            Self::MacosApp { display_path, .. } => display_path,
        }
    }
}

fn find_codex_launch_target() -> Option<CodexLaunchTarget> {
    #[cfg(target_os = "windows")]
    {
        if let Some(app_dir) =
            find_appx_codex_install_location().or_else(find_latest_packaged_codex_app_dir)
        {
            if let Some(app_user_model_id) = packaged_app_user_model_id(&app_dir) {
                return Some(CodexLaunchTarget::Packaged {
                    app_user_model_id,
                    display_path: app_dir.to_string_lossy().to_string(),
                });
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(app_dir) = find_macos_codex_app() {
            return Some(CodexLaunchTarget::MacosApp {
                display_path: app_dir.to_string_lossy().to_string(),
                app_dir: app_dir.to_string_lossy().to_string(),
            });
        }
    }

    find_installed_codex_executable().map(|path| CodexLaunchTarget::Executable {
        display_path: path.clone(),
        path,
    })
}

fn start_codex_launch_target(target: &CodexLaunchTarget, debug_port: u16) -> Result<(), String> {
    match target {
        CodexLaunchTarget::Executable { path, .. } => {
            start_codex_executable_with_debug_port(path, debug_port)
        }
        #[cfg(target_os = "windows")]
        CodexLaunchTarget::Packaged {
            app_user_model_id, ..
        } => activate_packaged_codex(app_user_model_id, debug_port).map(|_| ()),
        #[cfg(target_os = "macos")]
        CodexLaunchTarget::MacosApp { app_dir, .. } => start_macos_codex_app(app_dir, debug_port),
    }
}

fn start_codex_executable_with_debug_port(path: &str, debug_port: u16) -> Result<(), String> {
    let mut command = Command::new(path);
    command
        .arg(format!("--remote-debugging-port={debug_port}"))
        .arg(format!(
            "--remote-allow-origins=http://127.0.0.1:{debug_port}"
        ));
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn codex_debug_arguments(debug_port: u16) -> Vec<String> {
    vec![
        format!("--remote-debugging-port={debug_port}"),
        format!("--remote-allow-origins=http://127.0.0.1:{debug_port}"),
    ]
}

#[cfg(target_os = "windows")]
fn find_installed_codex_executable() -> Option<String> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let mut candidates = Vec::new();
    if let Some(local) = local {
        let codex_dir = local.join("OpenAI").join("Codex");
        candidates.push(codex_dir.join("Codex.exe"));
        candidates.push(codex_dir.join("codex.exe"));
        candidates.push(codex_dir.join("bin").join("Codex.exe"));
        candidates.push(codex_dir.join("bin").join("codex.exe"));
        if let Ok(entries) = std::fs::read_dir(codex_dir.join("bin")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    candidates.push(path.join("Codex.exe"));
                    candidates.push(path.join("codex.exe"));
                }
            }
        }
    }
    candidates
        .into_iter()
        .find(|path| path.exists())
        .map(|path| path.to_string_lossy().to_string())
}

#[cfg(target_os = "windows")]
fn find_appx_codex_install_location() -> Option<PathBuf> {
    let script = "(Get-AppxPackage -Name OpenAI.Codex -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty InstallLocation)";
    powershell_output(script)
        .ok()
        .map(|output| PathBuf::from(output.trim()))
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| {
            let app = path.join("app");
            if app.is_dir() {
                app
            } else {
                path
            }
        })
}

#[cfg(not(target_os = "windows"))]
fn find_installed_codex_executable() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        find_macos_codex_app()
            .map(|app| app.join("Contents").join("MacOS").join("Codex"))
            .filter(|path| path.exists())
            .map(|path| path.to_string_lossy().to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn find_macos_codex_app() -> Option<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        roots.push(home.join("Applications"));
    }
    for root in roots {
        for name in ["Codex.app", "OpenAI Codex.app", "OpenAI.Codex.app"] {
            let candidate = root.join(name);
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn start_macos_codex_app(app_dir: &str, debug_port: u16) -> Result<(), String> {
    let args = codex_debug_arguments(debug_port);
    Command::new("open")
        .arg("-na")
        .arg(app_dir)
        .arg("--args")
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn find_latest_packaged_codex_app_dir() -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        roots.push(PathBuf::from(program_files).join("WindowsApps"));
    }
    if let Some(program_files) = std::env::var_os("ProgramW6432") {
        roots.push(PathBuf::from(program_files).join("WindowsApps"));
    }
    roots.push(PathBuf::from(r"C:\Program Files\WindowsApps"));
    roots.sort();
    roots.dedup();

    roots
        .iter()
        .filter_map(|root| find_latest_codex_package_dir(root))
        .max_by(|left, right| version_tuple(left).cmp(&version_tuple(right)))
        .map(|package| {
            let app = package.join("app");
            if app.is_dir() {
                app
            } else {
                package
            }
        })
}

#[cfg(target_os = "windows")]
fn find_latest_codex_package_dir(root: &Path) -> Option<PathBuf> {
    std::fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("OpenAI.Codex_") && name.contains("__"))
        })
        .max_by(|left, right| version_tuple(left).cmp(&version_tuple(right)))
}

#[cfg(target_os = "windows")]
fn version_tuple(path: &Path) -> Option<(u32, u32, u32, u32)> {
    let name = path.file_name()?.to_str()?;
    let rest = name.strip_prefix("OpenAI.Codex_")?;
    let version = rest.split_once('_')?.0;
    let mut parts = version.split('.').map(str::parse::<u32>);
    Some((
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    ))
}

#[cfg(target_os = "windows")]
fn packaged_app_user_model_id(app_dir: &Path) -> Option<String> {
    let path = app_dir.to_string_lossy().replace('\\', "/");
    let mut parts = path.split('/').filter(|part| !part.is_empty());
    let mut package_name = parts.next_back()?;
    if package_name.eq_ignore_ascii_case("app") {
        package_name = parts.next_back()?;
    }
    if !package_name.starts_with("OpenAI.Codex_") || !package_name.contains("__") {
        return None;
    }
    let identity_name = package_name.split_once('_')?.0;
    let publisher_id = package_name.rsplit_once("__")?.1;
    (!publisher_id.is_empty()).then(|| format!("{identity_name}_{publisher_id}!App"))
}

#[cfg(target_os = "windows")]
fn activate_packaged_codex(app_user_model_id: &str, debug_port: u16) -> Result<u32, String> {
    let arguments = command_line_arguments(&codex_debug_arguments(debug_port));
    activate_packaged_app(app_user_model_id, &arguments)
}

#[cfg(target_os = "windows")]
fn activate_packaged_app(app_user_model_id: &str, arguments: &str) -> Result<u32, String> {
    let app_user_model_id = ps_single_quote(app_user_model_id);
    let arguments = ps_single_quote(arguments);
    let script = format!(
        r#"
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
[ComImport]
[Guid("2e941141-7f97-4756-ba1d-9decde894a3d")]
[InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IApplicationActivationManager {{
  IntPtr ActivateApplication([MarshalAs(UnmanagedType.LPWStr)] string appUserModelId, [MarshalAs(UnmanagedType.LPWStr)] string arguments, uint options, out uint processId);
}}
[ComImport]
[Guid("45BA127D-10A8-46EA-8AB7-56EA9078943C")]
class ApplicationActivationManager {{}}
public static class CodexToolkitActivator {{
  public static uint Activate(string appUserModelId, string arguments) {{
    var manager = (IApplicationActivationManager)new ApplicationActivationManager();
    uint processId;
    var hr = manager.ActivateApplication(appUserModelId, arguments, 0, out processId);
    if (hr != IntPtr.Zero) Marshal.ThrowExceptionForHR(hr.ToInt32());
    return processId;
  }}
}}
'@
[CodexToolkitActivator]::Activate('{app_user_model_id}', '{arguments}')
"#
    );
    powershell_output(&script)?
        .trim()
        .parse::<u32>()
        .map_err(|error| format!("Packaged Codex activation did not return a process id: {error}"))
}

#[cfg(target_os = "windows")]
fn ps_single_quote(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(target_os = "windows")]
fn command_line_arguments(args: &[String]) -> String {
    args.iter()
        .map(|arg| quote_windows_argument(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(target_os = "windows")]
fn quote_windows_argument(arg: &str) -> String {
    if !arg.is_empty() && !arg.bytes().any(|byte| matches!(byte, b' ' | b'\t' | b'"')) {
        return arg.to_string();
    }
    let mut output = String::from("\"");
    let mut backslashes = 0;
    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                output.push_str(&"\\".repeat(backslashes * 2 + 1));
                output.push('"');
                backslashes = 0;
            }
            _ => {
                output.push_str(&"\\".repeat(backslashes));
                output.push(ch);
                backslashes = 0;
            }
        }
    }
    output.push_str(&"\\".repeat(backslashes * 2));
    output.push('"');
    output
}

#[cfg(target_os = "windows")]
fn powershell_output(script: &str) -> Result<String, String> {
    let output = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            script,
        ])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err("PowerShell command failed.".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

const PLUGIN_UNLOCK_SCRIPT: &str = r#"
(() => {
  const selectors = {
    disabledInstallButton: 'button:disabled, button[aria-disabled="true"], [role="button"][aria-disabled="true"], button[data-disabled], [role="button"][data-disabled], button.cursor-not-allowed, [role="button"].cursor-not-allowed, button.pointer-events-none, [role="button"].pointer-events-none',
    pluginNavButton: 'nav[role="navigation"] button.h-token-nav-row.w-full',
    pluginSvgPath: 'svg path[d^="M7.94562 14.0277"]',
  };

  function reactFiberFrom(element) {
    const fiberKey = Object.keys(element).find((key) => key.startsWith("__reactFiber"));
    return fiberKey ? element[fiberKey] : null;
  }

  function authContextValueFrom(element) {
    for (let fiber = reactFiberFrom(element); fiber; fiber = fiber.return) {
      for (const value of [fiber.memoizedProps?.value, fiber.pendingProps?.value]) {
        if (value && typeof value === "object" && typeof value.setAuthMethod === "function" && "authMethod" in value) return value;
      }
    }
    return null;
  }

  function spoofChatGPTAuthMethod(element) {
    const auth = authContextValueFrom(element);
    if (!auth || auth.authMethod === "chatgpt") return false;
    auth.setAuthMethod("chatgpt");
    return true;
  }

  function pluginEntryButton() {
    const byIcon = document.querySelector(`${selectors.pluginNavButton} ${selectors.pluginSvgPath}`)?.closest("button");
    if (byIcon) return byIcon;
    return Array.from(document.querySelectorAll(selectors.pluginNavButton))
      .find((button) => /^(插件|Plugins)(\s+-\s+.*)?$/i.test((button.textContent || "").trim())) || null;
  }

  function patchReactDisabledProps(element) {
    Object.keys(element)
      .filter((key) => key.startsWith("__reactProps"))
      .forEach((key) => {
        const props = element[key];
        if (!props || typeof props !== "object") return;
        props.disabled = false;
        props["aria-disabled"] = false;
        props["data-disabled"] = undefined;
      });
  }

  function clearDisabledState(element) {
    if (!(element instanceof HTMLElement)) return;
    if ("disabled" in element) element.disabled = false;
    element.removeAttribute("disabled");
    element.removeAttribute("aria-disabled");
    element.removeAttribute("data-disabled");
    element.removeAttribute("inert");
    element.classList.remove("disabled", "opacity-50", "cursor-not-allowed", "pointer-events-none");
    element.style.pointerEvents = "auto";
    element.style.opacity = "";
    element.style.cursor = "pointer";
    element.tabIndex = 0;
    patchReactDisabledProps(element);
  }

  function enablePluginEntry() {
    const button = pluginEntryButton();
    if (!button) return;
    spoofChatGPTAuthMethod(button);
    clearDisabledState(button);
    button.style.display = "";
    button.querySelectorAll("*").forEach((node) => { node.style.display = ""; });
    if (button.dataset.codexToolkitPluginUnlocked === "true") return;
    button.dataset.codexToolkitPluginUnlocked = "true";
    button.addEventListener("click", () => spoofChatGPTAuthMethod(button), true);
  }

  function installButtonLabel(element) {
    return (element.textContent || "").trim();
  }

  function isInstallButtonLabel(text) {
    return /^安装\s*/.test(text) || /^Install\s*/i.test(text) || text === "强制安装";
  }

  function installButtonUnlockNodes(button) {
    const nodes = [button];
    button.querySelectorAll?.("button, [role='button'], [disabled], [aria-disabled], [data-disabled], .cursor-not-allowed, .pointer-events-none")
      .forEach((node) => nodes.push(node));
    let parent = button.parentElement;
    for (let depth = 0; parent && depth < 3; depth += 1, parent = parent.parentElement) {
      if (parent.matches?.("button, [role='button'], [disabled], [aria-disabled], [data-disabled], .cursor-not-allowed, .pointer-events-none")) nodes.push(parent);
    }
    return Array.from(new Set(nodes));
  }

  function unblockInstallButtons() {
    Array.from(document.querySelectorAll(selectors.disabledInstallButton))
      .map((node) => node.closest?.("button, [role='button']") || node)
      .filter((button, index, list) => list.indexOf(button) === index)
      .forEach((button) => {
        if (!isInstallButtonLabel(installButtonLabel(button))) return;
        installButtonUnlockNodes(button).forEach(clearDisabledState);
      });
  }

  function scan() {
    enablePluginEntry();
    unblockInstallButtons();
  }

  clearInterval(window.__codexToolkitPluginUnlockTimer);
  window.__codexToolkitPluginUnlockTimer = setInterval(scan, 1000);
  scan();
  window.__codexToolkitPluginUnlock = { installedAt: new Date().toISOString(), scan };
})();
"#;
