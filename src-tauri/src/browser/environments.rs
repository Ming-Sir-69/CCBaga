// [INPUT]: 显式环境编号、受限配置、本机引擎及资料占用核验；不读取 Claude 身份。
// [OUTPUT]: 可命名长期环境、原生启动/恢复与已有窗口激活、并行和一次性清理；窗口/后台/残留状态及确认退出后清理，不注入说明页或冒用实测结果。
// [POS]: 独立环境边界，不改身份选择、全局启动参数、实测报告、网络或默认浏览器。
// [PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md
use super::*;
use crate::identities::private;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
mod defaults;

const MANIFEST: &str = "environment.json";
const FORMAT: u32 = 1;
const START_GRACE_SECONDS: u64 = 30;

#[derive(Default)]
pub struct Environments {
    children: Mutex<HashMap<String, usize>>,
    errors: Mutex<HashMap<String, String>>,
    stopping: AtomicBool,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BrowserKind {
    #[default]
    Fingerprint,
    Brave,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Environment {
    format: u32,
    id: String,
    name: String,
    temporary: bool,
    cleanup_pending: bool,
    launched_at: Option<u64>,
    config: Config,
    #[serde(default)]
    browser: BrowserKind,
}

#[derive(Serialize)]
pub struct EnvironmentSummary {
    id: String,
    name: String,
    temporary: bool,
    running: bool,
    runtime_status: &'static str,
    process_count: Option<u32>,
    visible_window: Option<bool>,
    cleanup_pending: bool,
    error: Option<String>,
    config: Config,
    brave: bool,
}

#[derive(Serialize)]
pub struct EnvironmentSnapshot {
    items: Vec<EnvironmentSummary>,
    errors: Vec<EnvironmentIssue>,
}

#[derive(Serialize)]
pub struct EnvironmentIssue {
    id: String,
    error: String,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 80 || value.chars().any(char::is_control) {
        return Err("环境名称须为 1–80 个字符且不能包含控制字符。".into());
    }
    Ok(value.into())
}

fn environments_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(root(app)?.join("environments"))
}

fn directory(root: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("独立环境编号无效。".into());
    }
    let path = root.join(id);
    profile::check_path(root, &path)?;
    private::parents(&path)?;
    Ok(path)
}

fn load(root: &Path, id: &str) -> Result<Environment, String> {
    use std::io::Read;
    let dir = directory(root, id)?;
    private::safe(&dir)?;
    let path = dir.join(MANIFEST);
    private::safe(&path)?;
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .map_err(|_| "无法读取独立环境。")?
        .take(65_537)
        .read_to_end(&mut bytes)
        .map_err(|_| "无法读取独立环境。")?;
    if bytes.len() > 65_536 {
        return Err("独立环境配置过大。".into());
    }
    let value: Environment =
        serde_json::from_slice(&bytes).map_err(|_| "独立环境配置损坏，未覆盖。")?;
    if value.format != FORMAT
        || value.id != id
        || name(&value.name)? != value.name
        || value.config.seed == 0
    {
        return Err("独立环境配置无效，未修改资料。".into());
    }
    config::proxy(&value.config.proxy)?;
    Ok(value)
}

fn save(root: &Path, value: &Environment) -> Result<(), String> {
    let dir = directory(root, &value.id)?;
    private::safe(&dir)?;
    let file = dir.join(MANIFEST);
    profile::check_path(root, &file)?;
    private::write(
        &file,
        &serde_json::to_vec_pretty(value).map_err(|_| "环境配置编码失败。")?,
    )
}

fn inventory(root: &Path) -> Result<(Vec<Environment>, Vec<EnvironmentIssue>), String> {
    profile::check_path(root, root)?;
    if !root.exists() {
        return Ok((Vec::new(), Vec::new()));
    }
    private::safe(root)?;
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for entry in fs::read_dir(root).map_err(|_| "无法读取独立环境列表。")? {
        let entry = entry.map_err(|_| "无法读取独立环境列表。")?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if valid_id(&id) {
            match load(root, &id) {
                Ok(value) => items.push(value),
                Err(error) => errors.push(EnvironmentIssue { id, error }),
            }
        }
    }
    items.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok((items, errors))
}

fn list(root: &Path) -> Result<Vec<Environment>, String> {
    inventory(root).map(|(items, _)| items)
}

fn create(
    root: &Path,
    label: &str,
    temporary: bool,
    config: Option<Config>,
    machine: &Machine,
) -> Result<Environment, String> {
    let name = name(label)?;
    let used: HashSet<u32> = list(root)?.iter().map(|e| e.config.seed).collect();
    let config = match config {
        Some(config) => config,
        None => {
            let mut config = Config::default();
            // Generate rather than reuse the same default seed for every environment.
            loop {
                let random = server::nonce()?;
                config.seed =
                    u32::from_str_radix(&random[..8], 16).map_err(|_| "无法创建环境种子。")?;
                if config.seed != 0 && !used.contains(&config.seed) {
                    break;
                }
            }
            config
        }
    };
    config.validate(machine)?;
    let id = server::nonce()?[..32].to_string();
    let dir = directory(root, &id)?;
    private::directory(root)?;
    private::create_directory(&dir)?;
    let value = Environment {
        format: FORMAT,
        id,
        name,
        temporary,
        cleanup_pending: false,
        launched_at: None,
        config,
        browser: BrowserKind::Fingerprint,
    };
    if let Err(error) = save(root, &value) {
        let _ = fs::remove_dir(&dir);
        return Err(error);
    }
    Ok(value)
}

fn validate_brave_profile(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("Brave 资料目录必须是绝对路径。".into());
    }
    private::parents(path)?;
    private::safe(path)?;
    for child in [path.join("Default"), path.join("Local State")] {
        profile::check_path(path, &child)?;
        private::safe(&child)?;
    }
    if !path.is_dir() || !path.join("Default").is_dir() || !path.join("Local State").is_file() {
        return Err("请选择包含 Default 与 Local State 的 Brave 资料目录。".into());
    }
    Ok(())
}

fn brave_launch_arguments(path: &Path) -> Vec<String> {
    vec!["-n".into(), "-a".into(), "/Applications/Brave Browser.app".into(), "--args".into(),
        format!("--user-data-dir={}", path.display()), "--profile-directory=Default".into(),
        "--no-first-run".into(), "--no-default-browser-check".into(), "--new-window".into(), "brave://newtab/".into()]
}

fn open_brave_profile(path: &Path) -> Result<(), String> {
    validate_brave_profile(path)?;
    let activity = profile::activity(path, true)?;
    if !activity.main_running && activity.busy() { return Err("后台进程尚未退出，请稍后重新检查。".into()); }
    if activity.main_running && profile::brave_action(path, "activate")? { return Ok(()); }
    #[cfg(target_os = "macos")]
    {
        if !Path::new("/Applications/Brave Browser.app/Contents/MacOS/Brave Browser").is_file() {
            return Err("未找到原 Brave 程序，未改用其他内核。".into());
        }
        if !Command::new("/usr/bin/open").args(brave_launch_arguments(path)).status().map_err(|_| "Brave 启动失败。")?.success() {
            return Err("Brave 启动失败。".into());
        }
        for _ in 0..100 {
            if profile::brave_action(path, "running")? { return Ok(()); }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err("Brave 仍在启动，请刷新状态。".into())
    }
    #[cfg(not(target_os = "macos"))]
    { Err("现有 Brave 接管目前仅支持 macOS。".into()) }
}

fn environment_running(profile: &Path, item: &Environment) -> Result<bool, String> {
    profile::activity(profile, item.browser == BrowserKind::Brave).map(|a| a.busy())
}

fn owned_child(app: &tauri::AppHandle, id: &str) -> bool {
    app.state::<Environments>()
        .children
        .lock()
        .unwrap()
        .contains_key(id)
}

fn ensure_stopped(app: &tauri::AppHandle, id: &str, profile: &Path) -> Result<(), String> {
    let item = load(&environments_root(app)?, id)?;
    if owned_child(app, id) || environment_running(profile, &item)? {
        return Err("独立环境仍在运行或启动，请先正常退出。".into());
    }
    if starting(&item, now()) {
        return Err("环境启动状态尚未确认，请稍后重试。".into());
    }
    Ok(())
}

fn lifecycle(activity: Option<&profile::Activity>, owned: bool, starting: bool) -> &'static str {
    let Some(a) = activity else { return "unknown"; };
    if a.main_running {
        return match a.visible_window { Some(true) => "open", Some(false) => "background", None => "running" };
    }
    if a.process_count > 0 { return "residual"; }
    if owned { return if starting { "starting" } else { "closing" }; }
    if starting { return "checking"; }
    "stopped"
}

fn snapshot(app: &tauri::AppHandle) -> Result<EnvironmentSnapshot, String> {
    let root = environments_root(app)?;
    let mut items = Vec::new();
    let (environments, errors) = inventory(&root)?;
    for value in environments {
        let result = profile::activity(&directory(&root, &value.id)?.join("profile"), value.browser == BrowserKind::Brave);
        let observation = result.as_ref().ok();
        let runtime_status = lifecycle(observation, owned_child(app, &value.id), starting(&value, now()));
        // Unknown observations must never enable deletion or pretend that an environment stopped.
        let running = runtime_status != "stopped";
        let process_count = observation.map(|a| a.process_count);
        let visible_window = observation.and_then(|a| a.visible_window);
        let error = app
            .state::<Environments>()
            .errors
            .lock()
            .unwrap()
            .get(&value.id)
            .cloned().or_else(|| result.err());
        items.push(EnvironmentSummary {
            brave: value.browser == BrowserKind::Brave,
            runtime_status,
            process_count,
            visible_window,
            id: value.id,
            name: value.name,
            temporary: value.temporary,
            running,
            cleanup_pending: value.cleanup_pending,
            error,
            config: value.config,
        });
    }
    Ok(EnvironmentSnapshot { items, errors })
}

fn remove(
    root: &Path,
    id: &str,
    mut stopped: impl FnMut() -> Result<(), String>,
) -> Result<(), String> {
    let load_before_remove = load(root, id)?;
    let dir = directory(root, id)?;
    let profile = dir.join("profile");
    stopped()?;
    // Preflight the entire envelope before deleting any profile data. Unknown entries are retained.
    for entry in fs::read_dir(&dir).map_err(|_| "无法检查独立环境目录。")? {
        let entry = entry.map_err(|_| "无法检查独立环境目录。")?;
        if entry.file_name() != MANIFEST && entry.file_name() != "profile" {
            return Err("独立环境包含未知文件，未删除。".into());
        }
        private::safe(&entry.path())?;
    }
    profile::reset(&profile, &mut stopped, |_, _, _, _| {})?;
    stopped()?;
    if profile.exists() {
        fs::remove_dir(&profile).map_err(|_| "环境资料未完全删除。")?;
    }
    private::safe(&dir.join(MANIFEST))?;
    fs::remove_file(dir.join(MANIFEST)).map_err(|_| "环境资料已清理，配置未能删除。")?;
    if fs::remove_dir(&dir).is_err() {
        // Keep a discoverable manifest when a concurrent filesystem change prevents final removal.
        let _ = save(root, &load_before_remove);
        return Err("环境资料已清理，空目录未能删除。".into());
    }
    private::sync_parent(&dir);
    Ok(())
}

fn cleanup(app: &tauri::AppHandle) -> Result<(), String> {
    let root = environments_root(app)?;
    for item in list(&root)? {
        if !cleanup_candidate(&item, now()) {
            continue;
        }
        let profile = directory(&root, &item.id)?.join("profile");
        if owned_child(app, &item.id) || environment_running(&profile, &item)? {
            continue;
        }
        let result = remove(&root, &item.id, || ensure_stopped(app, &item.id, &profile));
        let state = app.state::<Environments>();
        let mut errors = state.errors.lock().unwrap();
        match result {
            Ok(()) => {
                errors.remove(&item.id);
            }
            Err(error) => {
                errors.insert(item.id, error);
            }
        }
    }
    Ok(())
}

fn cleanup_candidate(item: &Environment, time: u64) -> bool {
    item.temporary
        && item.cleanup_pending
        // A host crash during spawn may precede Chromium's lock. Do not race a fresh launch.
        && !starting(item, time)
}

fn starting(item: &Environment, time: u64) -> bool {
    item.launched_at
        .is_some_and(|started| time.saturating_sub(started) < START_GRACE_SECONDS)
}

fn reap(app: &tauri::AppHandle, id: &str, mut child: std::process::Child) {
    let handle = app.clone();
    let owned_id = id.to_string();
    std::thread::spawn(move || {
        let _ = child.wait();
        let state = handle.state::<Environments>();
        {
            let mut children = state.children.lock().unwrap();
            if let Some(count) = children.get_mut(&owned_id) {
                *count -= 1;
                if *count == 0 {
                    children.remove(&owned_id);
                }
            }
        }
        let browser = handle.state::<Browser>();
        let _guard = browser.gate.lock().unwrap();
        let _ = (|| -> Result<(), String> {
            let root = environments_root(&handle)?;
            let mut item = load(&root, &owned_id)?;
            let profile = directory(&root, &owned_id)?.join("profile");
            // A relay may have exited while the real browser is still alive.
            if !owned_child(&handle, &owned_id) && !environment_running(&profile, &item)? {
                item.launched_at = None;
                save(&root, &item)?;
                cleanup(&handle)?;
            }
            Ok(())
        })();
    });
}

fn track(app: &tauri::AppHandle, id: &str) {
    *app.state::<Environments>()
        .children
        .lock()
        .unwrap()
        .entry(id.into())
        .or_default() += 1;
}

pub fn monitor(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        if app.state::<Environments>().stopping.load(Ordering::Relaxed) {
            break;
        }
        let browser = app.state::<Browser>();
        let Ok(_guard) = browser.gate.try_lock() else {
            continue;
        };
        // Corrupt environments are retained and reported; other environments remain independently cleanable.
        let _ = cleanup(&app);
    });
}

pub fn stop(app: &tauri::AppHandle) {
    app.state::<Environments>()
        .stopping
        .store(true, Ordering::Relaxed);
}

#[tauri::command]
pub async fn browser_environments_read(
    app: tauri::AppHandle,
) -> Result<EnvironmentSnapshot, String> {
    run(move || {
        let browser = app.state::<Browser>();
        let _guard = browser.gate.lock().unwrap();
        snapshot(&app)
    })
    .await
}

#[tauri::command]
pub async fn browser_environment_create(
    app: tauri::AppHandle,
    name: String,
    temporary: bool,
    config: Option<Config>,
) -> Result<EnvironmentSnapshot, String> {
    run(move || {
        let browser = app.state::<Browser>();
        let _guard = browser.gate.lock().unwrap();
        create(
            &environments_root(&app)?,
            &name,
            temporary,
            config,
            &config::machine()?,
        )?;
        snapshot(&app)
    })
    .await
}

#[tauri::command]
pub async fn browser_environment_rename(
    app: tauri::AppHandle,
    id: String,
    name: String,
) -> Result<EnvironmentSnapshot, String> {
    run(move || {
        let browser = app.state::<Browser>();
        let _guard = browser.gate.lock().unwrap();
        rename(&environments_root(&app)?, &id, &name)?;
        snapshot(&app)
    })
    .await
}

fn rename(root: &Path, id: &str, label: &str) -> Result<(), String> {
    let mut item = load(root, id)?;
    item.name = name(label)?;
    // Display metadata only: do not touch live Chromium preferences or authentication state.
    save(root, &item)
}

#[tauri::command]
pub async fn browser_environment_save(
    app: tauri::AppHandle,
    id: String,
    name: String,
    config: Config,
) -> Result<EnvironmentSnapshot, String> {
    run(move || {
        let browser = app.state::<Browser>();
        let _guard = browser.gate.lock().unwrap();
        let root = environments_root(&app)?;
        let mut item = load(&root, &id)?;
        if item.browser == BrowserKind::Brave { return Err("此环境沿用 Brave 原设置，请在浏览器中配置。".into()); }
        if item.cleanup_pending {
            return Err("一次性环境已结束使用，请清理后新建。".into());
        }
        ensure_stopped(&app, &id, &directory(&root, &id)?.join("profile"))?;
        item.name = self::name(&name)?;
        config.validate(&config::machine()?)?;
        item.config = config;
        save(&root, &item)?;
        snapshot(&app)
    })
    .await
}

fn name_profile(profile: &Path, item: &Environment) -> Result<(), String> {
    let prefs = profile.join("Default/Preferences");
    profile::check_path(profile, &prefs)?;
    let mut value: serde_json::Value = serde_json::from_slice(
        &fs::read(&prefs).map_err(|_| "无法读取环境偏好。")?,
    ).map_err(|_| "环境偏好格式无效。")?;
    value["profile"]["name"] = serde_json::json!(item.name);
    // Preserve Chromium's startup/session settings and all saved site state.
    profile::write_json(&prefs, &value)
}

#[derive(Debug, PartialEq)]
enum StartupPlan {
    Google,
    Restore,
    Native,
    Pages(Vec<String>),
}

fn startup_plan_from_values(ordinary: &serde_json::Value, secure: &serde_json::Value) -> Result<StartupPlan, String> {
    let field = |key: &str| secure.pointer(key).or_else(|| ordinary.pointer(key));
    let mode = field("/session/restore_on_startup").map(|v| v.as_u64().ok_or("浏览器启动设置无效，未打开窗口。" )).transpose()?;
    match mode {
        Some(1) => Ok(StartupPlan::Restore),
        Some(4) => {
            let Some(value) = field("/session/startup_urls") else { return Ok(StartupPlan::Google); };
            let urls = value.as_array().ok_or("浏览器启动设置无效，未打开窗口。")?;
            if urls.len() > 128 { return Err("浏览器启动设置无效，未打开窗口。".into()); }
            let mut pages = Vec::new();
            for value in urls {
                let url = value.as_str().ok_or("浏览器启动设置无效，未打开窗口。")?;
                if url.len() > 8192 || url.chars().any(char::is_control) {
                    return Err("浏览器启动设置无效，未打开窗口。".into());
                }
                let parsed = reqwest::Url::parse(url).map_err(|_| "浏览器启动设置无效，未打开窗口。")?;
                if !["http", "https", "file", "chrome", "chrome-extension", "about"].contains(&parsed.scheme()) {
                    return Err("浏览器启动设置无效，未打开窗口。".into());
                }
                pages.push(url.to_string());
            }
            Ok(if pages.is_empty() { StartupPlan::Native } else { StartupPlan::Pages(pages) })
        }
        Some(_) => Ok(StartupPlan::Native),
        None => Ok(StartupPlan::Google),
    }
}

fn startup_plan(profile: &Path) -> Result<StartupPlan, String> {
    use std::io::Read;
    let read = |name: &str| -> Result<serde_json::Value, String> {
        let file = profile.join("Default").join(name);
        profile::check_path(profile, &file)?;
        if !file.exists() { return Ok(serde_json::Value::Null); }
        private::safe(&file)?;
        let mut bytes = Vec::new();
        fs::File::open(&file).map_err(|_| "无法读取浏览器初始设置。")?
            .take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes).map_err(|_| "无法读取浏览器初始设置。")?;
        if bytes.len() > 32 * 1024 * 1024 { return Err("浏览器初始设置文件过大，未修改。".into()); }
        serde_json::from_slice(&bytes).map_err(|_| "浏览器书签或偏好损坏，未覆盖。".into())
    };
    // Only read startup preferences. Never write protected preferences or their MACs.
    startup_plan_from_values(&read("Preferences")?, &read("Secure Preferences")?)
}

fn window_arguments(
    config: &Config,
    machine: &Machine,
    profile: &Path,
    version: &str,
    running_without_window: bool,
    startup: &StartupPlan,
) -> Vec<String> {
    let mut args = config::arguments(config, machine, profile, "", version);
    args.pop();
    args.push("--disable-background-mode".into());
    if running_without_window {
        args.extend(["--new-window".into(), defaults::START_PAGE.into()]);
    } else {
        // macOS Chromium can initialize a process without a visible window when
        // started directly. Create its first window, except when restoring the
        // previous session; an already-visible process returned before this path.
        if !matches!(startup, StartupPlan::Restore) {
            args.push("--new-window".into());
        }
        match startup {
            StartupPlan::Google => args.push(defaults::START_PAGE.into()),
            StartupPlan::Restore => args.push("--restore-last-session".into()),
            StartupPlan::Native => {},
            StartupPlan::Pages(pages) => args.extend(pages.iter().cloned()),
        }
    }
    args
}

fn launch(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let root = environments_root(app)?;
    let mut item = load(&root, id)?;
    let profile = directory(&root, id)?.join("profile");
    if item.browser == BrowserKind::Brave { return open_brave_profile(&profile); }
    let activity = profile::activity(&profile, false)?;
    if !activity.main_running && activity.busy() { return Err("后台进程尚未退出，请稍后重新检查。".into()); }
    let running = activity.main_running;
    if running && profile::activate_window(&profile)? {
        return Ok(());
    }
    if !running {
        ensure_stopped(app, id, &profile)?;
        if item.cleanup_pending {
            return Err("一次性环境已结束使用，请清理后新建。".into());
        }
    }
    let machine = config::machine()?;
    item.config.validate(&machine)?;
    let engine = prepare_engine(app)?;
    let binary = binary(&engine);
    if !binary.is_file() {
        return Err("内置浏览器引擎不可用。".into());
    }
    if !running {
        private::directory(&profile)?;
        profile::preferences(&profile, &item.config, &machine)?;
        name_profile(&profile, &item)?;
        defaults::apply(&profile, || ensure_stopped(app, id, &profile))?;
    }
    item.cleanup_pending = item.temporary;
    item.launched_at = Some(now());
    save(&root, &item)?;
    let startup = if running { StartupPlan::Google } else { startup_plan(&profile)? };
    let args = window_arguments(
        &item.config,
        &machine,
        &profile,
        &version(&engine),
        running,
        &startup,
    );
    // No shell, host launch files or companion report token: each process owns only its profile.
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "独立环境启动失败。")?;
    track(app, id);
    app.state::<Environments>()
        .errors
        .lock()
        .unwrap()
        .remove(id);
    if running {
        // Chromium forwards this request to the existing profile process. Reap the relay separately.
        reap(app, id, child);
        return Ok(());
    }
    let result = (|| {
        for _ in 0..100 {
            if profile::running(&profile)? {
                return Ok(());
            }
            if child
                .try_wait()
                .map_err(|_| "无法读取独立环境进程状态。")?
                .is_some()
            {
                return Err("独立环境启动后退出，请检查浏览器引擎。".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err("独立环境仍在启动，请刷新状态。".to_string())
    })();
    reap(app, id, child);
    result
}

#[tauri::command]
pub async fn browser_environment_launch(
    app: tauri::AppHandle,
    id: String,
) -> Result<EnvironmentSnapshot, String> {
    run(move || {
        let browser = app.state::<Browser>();
        let _guard = browser.gate.lock().unwrap();
        launch(&app, &id)?;
        snapshot(&app)
    })
    .await
}

#[tauri::command]
pub async fn browser_environment_quit(
    app: tauri::AppHandle,
    id: String,
) -> Result<EnvironmentSnapshot, String> {
    run(move || {
        let browser = app.state::<Browser>();
        let _guard = browser.gate.lock().unwrap();
        let root = environments_root(&app)?;
        let mut item = load(&root, &id)?;
        if item.browser == BrowserKind::Brave {
            let path = directory(&root, &id)?.join("profile");
            validate_brave_profile(&path)?;
            profile::brave_action(&path, "quit")?;
            for _ in 0..100 {
                if !environment_running(&path, &item)? { return snapshot(&app); }
                std::thread::sleep(Duration::from_millis(50));
            }
            return Err("当前环境仍有进程未退出，请检查浏览器提示后重试。".into());
        }
        let profile = directory(&root, &id)?.join("profile");
        if !profile::running(&profile)? && starting(&item, now()) {
            return Err("环境启动状态尚未确认，请稍后重试。".into());
        }
        profile::quit(&profile)?;
        for _ in 0..100 {
            if !owned_child(&app, &id) && !environment_running(&profile, &item)? {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if owned_child(&app, &id) || environment_running(&profile, &item)? {
            return Err("当前环境仍有进程未退出，请检查浏览器提示后重试。".into());
        }
        item.launched_at = None;
        save(&root, &item)?;
        if item.temporary {
            // Explicit normal exit removes the startup grace; failures remain retryable.
            item.launched_at = None;
            item.cleanup_pending = true;
            save(&root, &item)?;
            cleanup(&app)?;
        }
        snapshot(&app)
    })
    .await
}

#[tauri::command]
pub async fn browser_environment_delete(
    app: tauri::AppHandle,
    id: String,
) -> Result<EnvironmentSnapshot, String> {
    run(move || {
        let browser = app.state::<Browser>();
        let _guard = browser.gate.lock().unwrap();
        let root = environments_root(&app)?;
        let profile = directory(&root, &id)?.join("profile");
        remove(&root, &id, || ensure_stopped(&app, &id, &profile))?;
        app.state::<Environments>()
            .errors
            .lock()
            .unwrap()
            .remove(&id);
        snapshot(&app)
    })
    .await
}

#[tauri::command]
pub async fn browser_environments_cleanup(
    app: tauri::AppHandle,
) -> Result<EnvironmentSnapshot, String> {
    run(move || {
        let browser = app.state::<Browser>();
        let _guard = browser.gate.lock().unwrap();
        cleanup(&app)?;
        snapshot(&app)
    })
    .await
}

#[cfg(test)]
mod tests;
