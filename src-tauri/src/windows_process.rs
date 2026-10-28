// [INPUT]: Win32 Toolhelp/进程句柄、当前用户 SID、精确 CLI 入口与独立浏览器资料路径。
// [OUTPUT]: 官方 CLI/Desktop 精确进程与句柄复核后的结束；浏览器限本人固定引擎、唯一绝对资料参数和同目录句柄标识，读取未知阻止修改。
// [POS]: Windows 进程边界；参数仅用于原生判定，不返回 UI；不对其他用户或名称相似的程序执行终止。
// [PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md
use super::{current_sid, input, process_sid, response};
use crate::cleaner::ClaudeProcess;
use serde_json::json;
use std::{ffi::c_char, path::Path};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES,
        FILETIME, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM, STILL_ACTIVE,
    },
    System::{
        Diagnostics::{
            Debug::ReadProcessMemory,
            ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
        },
        Threading::{
            GetExitCodeProcess, GetProcessTimes, IsWow64Process, OpenProcess,
            QueryFullProcessImageNameW, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_TERMINATE, PROCESS_VM_READ,
        },
    },
    UI::{
        Shell::CommandLineToArgvW,
        WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE},
    },
};
#[path = "windows_browser_arguments.rs"]
mod browser_arguments;
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
impl Handle {
    fn open(pid: u32, terminate: bool) -> Option<Self> {
        let flags = PROCESS_QUERY_LIMITED_INFORMATION
            | PROCESS_VM_READ
            | if terminate { PROCESS_TERMINATE } else { 0 };
        let handle = unsafe { OpenProcess(flags, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let handle = Self(handle);
        if unsafe { process_sid(handle.0) }.as_ref() != current_sid() || current_sid().is_none() {
            return None;
        }
        Some(handle)
    }
    fn image(&self) -> Option<String> {
        let mut buffer = vec![0u16; 32768];
        let mut size = buffer.len() as u32;
        if unsafe { QueryFullProcessImageNameW(self.0, 0, buffer.as_mut_ptr(), &mut size) } == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..size as usize]))
    }
    fn packaged_claude(&self) -> bool {
        let mut buffer = [0u16; 128];
        let mut size = buffer.len() as u32;
        let status = unsafe {
            windows_sys::Win32::Storage::Packaging::Appx::GetPackageFamilyName(
                self.0,
                &mut size,
                buffer.as_mut_ptr(),
            )
        };
        status == 0
            && String::from_utf16_lossy(&buffer[..size.saturating_sub(1) as usize])
                == "Claude_pzs8sxrjxfjjc"
    }
    fn created(&self) -> Option<u64> {
        let [mut created, mut exited, mut kernel, mut user] = [FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        }; 4];
        if unsafe { GetProcessTimes(self.0, &mut created, &mut exited, &mut kernel, &mut user) }
            == 0
        {
            return None;
        }
        Some((created.dwHighDateTime as u64) << 32 | created.dwLowDateTime as u64)
    }
}
#[repr(C)]
#[derive(Default, Copy, Clone)]
struct BasicInfo {
    status: i32,
    peb: usize,
    affinity: usize,
    priority: i32,
    pid: usize,
    parent: usize,
}
#[repr(C)]
#[derive(Default, Copy, Clone)]
struct Unicode {
    length: u16,
    maximum: u16,
    buffer: usize,
}
#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationProcess(
        handle: HANDLE,
        class: u32,
        info: *mut std::ffi::c_void,
        size: u32,
        written: *mut u32,
    ) -> i32;
}
fn read<T: Default + Copy>(handle: HANDLE, address: usize) -> Option<T> {
    let mut value = T::default();
    let mut written = 0;
    let size = std::mem::size_of::<T>();
    if address == 0
        || unsafe {
            ReadProcessMemory(
                handle,
                address as _,
                (&mut value as *mut T).cast(),
                size,
                &mut written,
            )
        } == 0
        || written != size
    {
        return None;
    }
    Some(value)
}
fn arguments(handle: &Handle) -> Option<Vec<String>> {
    unsafe {
        let mut wow = 0;
        if IsWow64Process(handle.0, &mut wow) == 0 || wow != 0 {
            return None;
        }
        let mut basic = BasicInfo::default();
        let mut written = 0;
        if NtQueryInformationProcess(
            handle.0,
            0,
            (&mut basic as *mut BasicInfo).cast(),
            std::mem::size_of::<BasicInfo>() as u32,
            &mut written,
        ) != 0
        {
            return None;
        }
        let params: usize = read(handle.0, basic.peb.checked_add(0x20)?)?;
        let command: Unicode = read(handle.0, params.checked_add(0x70)?)?;
        if command.buffer == 0
            || command.length == 0
            || command.length > command.maximum
            || command.length % 2 != 0
        {
            return None;
        }
        let mut buffer = vec![0u16; command.length as usize / 2 + 1];
        let mut read_bytes = 0usize;
        if ReadProcessMemory(
            handle.0,
            command.buffer as _,
            buffer.as_mut_ptr().cast(),
            command.length as usize,
            &mut read_bytes,
        ) == 0
            || read_bytes != command.length as usize
        {
            return None;
        }
        parsed_arguments(&buffer[..command.length as usize / 2])
    }
}
// ----- 两条原生读取路径共用 Windows 参数语法；完整参数判定，禁止子串命中 -----
fn parsed_arguments(command: &[u16]) -> Option<Vec<String>> {
    let buffer: Vec<u16> = command.iter().copied().chain(Some(0)).collect();
    unsafe {
        let mut count = 0;
        let args = CommandLineToArgvW(buffer.as_ptr(), &mut count);
        if args.is_null() {
            return None;
        }
        let values = std::slice::from_raw_parts(args, count as usize)
            .iter()
            .map(|p| {
                let mut size = 0;
                while *p.add(size) != 0 {
                    size += 1;
                }
                String::from_utf16_lossy(std::slice::from_raw_parts(*p, size))
            })
            .collect();
        LocalFree(args.cast());
        Some(values)
    }
}
fn normalized(path: &str) -> String {
    path.replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}
fn official_cli(args: &[String]) -> bool {
    let script = if args
        .first()
        .is_some_and(|a| normalized(a).ends_with("\\bun.exe"))
        && args.get(1).is_some_and(|a| a == "run")
    {
        2
    } else {
        1
    };
    args.get(script).is_some_and(|arg| {
        Path::new(arg).is_absolute()
            && normalized(arg).ends_with("\\node_modules\\@anthropic-ai\\claude-code\\cli.js")
    })
}
fn claude_kind(image: &str, args: &[String]) -> Option<&'static str> {
    let path = normalized(image);
    let name = path.rsplit('\\').next()?;
    if ["node.exe", "bun.exe"].contains(&name) && official_cli(args) {
        return Some("Claude Code");
    }
    let home = normalized(&std::env::var("USERPROFILE").ok()?);
    if path == format!("{home}\\.local\\bin\\claude.exe")
        || path == format!("{home}\\.bun\\bin\\claude.exe")
        || (Path::new(&path).parent()
            == Some(Path::new(&format!(
                "{home}\\.local\\share\\claude\\versions"
            )))
            && version_name(name))
    {
        return Some("Claude Code");
    }
    let local = normalized(&std::env::var("LOCALAPPDATA").ok()?);
    if path.starts_with(&format!("{local}\\anthropicclaude\\"))
        || path.starts_with(&format!("{local}\\programs\\claude\\"))
    {
        if [
            "claude.exe",
            "cowork-svc.exe",
            "claude-ssh-askpass.exe",
            "claude-ssh-proxy.exe",
        ]
        .contains(&name)
        {
            return Some("Claude Desktop");
        }
    }
    if name != "claude.exe" {
        return None;
    }
    if path == format!("{local}\\microsoft\\winget\\packages\\anthropic.claudecode_microsoft.winget.source_8wekyb3d8bbwe\\claude.exe")
        || path == format!("{local}\\microsoft\\winget\\links\\claude.exe")
    {
        return Some("Claude Code");
    }
    None
}
fn version_name(name: &str) -> bool {
    let name = name.strip_suffix(".exe").unwrap_or(name);
    let parts: Vec<_> = name.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}
fn processes(terminate: bool) -> Result<Vec<(u32, Handle)>, String> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err("无法检查 Windows 进程。".into());
    }
    let snapshot = Handle(snapshot);
    let mut entry = unsafe { std::mem::zeroed::<PROCESSENTRY32W>() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut result = vec![];
    let mut has = unsafe { Process32FirstW(snapshot.0, &mut entry) };
    while has != 0 {
        let name = String::from_utf16_lossy(&entry.szExeFile)
            .trim_end_matches('\0')
            .to_lowercase();
        if ([
            "claude.exe",
            "cowork-svc.exe",
            "claude-ssh-askpass.exe",
            "claude-ssh-proxy.exe",
            "node.exe",
            "bun.exe",
            "chrome.exe",
        ]
        .contains(&name.as_str())
            || version_name(&name))
            && entry.th32ProcessID > 1
        {
            if let Some(handle) = Handle::open(entry.th32ProcessID, terminate) {
                result.push((entry.th32ProcessID, handle));
            }
        }
        has = unsafe { Process32NextW(snapshot.0, &mut entry) };
    }
    Ok(result)
}
pub fn claude_processes() -> Result<Vec<ClaudeProcess>, String> {
    let mut found = vec![];
    for (pid, handle) in processes(false)? {
        let Some(image) = handle.image() else {
            continue;
        };
        let args = arguments(&handle).unwrap_or_default();
        let kind = if handle.packaged_claude() {
            Some("Claude Desktop")
        } else {
            claude_kind(&image, &args)
        };
        if let (Some(kind), Some(created)) = (kind, handle.created()) {
            found.push(ClaudeProcess {
                pid: pid as i32,
                name: kind.into(),
                executable: image,
                created,
            });
        }
    }
    Ok(found)
}
pub fn kill_process(target: &ClaudeProcess) -> Result<(), String> {
    let Some(handle) = Handle::open(target.pid as u32, true) else {
        return Ok(());
    };
    if handle.created() != Some(target.created)
        || handle.image().as_deref() != Some(target.executable.as_str())
    {
        return Ok(());
    }
    if !handle.packaged_claude()
        && claude_kind(&target.executable, &arguments(&handle).unwrap_or_default()).is_none()
    {
        return Ok(());
    }
    if unsafe { TerminateProcess(handle.0, 1) } == 0 {
        return Err("无法终止已核验的 Claude 进程。".into());
    }
    Ok(())
}
fn browser_unknown() -> String {
    "无法确认浏览器进程的资料归属，未操作资料。".into()
}
// ----- 参数只有一个明确绝对资料目录；“--”后的文本属于页面参数 -----
fn profile_argument(args: &[String]) -> Option<&Path> {
    let mut args = args.iter().skip(1);
    let mut selected = None;
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        let value = if arg == "--user-data-dir" {
            Some(args.next()?.as_str())
        } else {
            arg.strip_prefix("--user-data-dir=")
        };
        if let Some(value) = value {
            if selected.is_some() || !Path::new(value).is_absolute() {
                return None;
            }
            selected = Some(Path::new(value));
        }
    }
    selected
}
// ----- 跟随目录别名后比较实际对象，避免 verbatim、8.3 或 junction 名称误判 -----
fn file_identity(path: &Path, directory: bool) -> Result<(u32, u64), String> {
    use windows_sys::Win32::Storage::FileSystem::*;
    let wide = super::wide_path(path);
    let raw = unsafe {
        CreateFileW(
            wide.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(browser_unknown());
    }
    let handle = Handle(raw);
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(handle.0, &mut info) } == 0
        || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
    {
        return Err(browser_unknown());
    }
    Ok((
        info.dwVolumeSerialNumber,
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    ))
}
fn profile_matches(args: &[String], profile: &str) -> Result<bool, String> {
    let selected = profile_argument(args).ok_or_else(browser_unknown)?;
    Ok(file_identity(selected, true)? == file_identity(Path::new(profile), true)?)
}
// 只对本应用固定安装/开发引擎读取 argv；普通 Chrome 的权限失败不阻断本应用。
fn companion_image(image: &str) -> bool {
    // 已安装与开发副本可同时存在；两者都沿用同一 WindowsBrowser 载荷目录。
    if Path::new(image)
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| {
            name.to_string_lossy()
                .eq_ignore_ascii_case("WindowsBrowser")
        })
    {
        return true;
    }
    let mut expected = vec![Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("browser-runtime/WindowsBrowser/chrome.exe")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            expected.extend([
                parent.join("browser-runtime/WindowsBrowser/chrome.exe"),
                parent.join("WindowsBrowser/chrome.exe"),
            ]);
        }
    }
    expected.iter().any(|path| {
        normalized(&path.to_string_lossy()) == normalized(image)
            || file_identity(path, false)
                .is_ok_and(|expected| file_identity(Path::new(image), false) == Ok(expected))
    })
}
unsafe extern "system" fn close_window(window: HWND, pid: LPARAM) -> i32 {
    let mut owner = 0;
    GetWindowThreadProcessId(window, &mut owner);
    if owner == pid as u32 {
        PostMessageW(window, WM_CLOSE, 0, 0);
    }
    1
}
fn process_exited(handle: &Handle) -> Result<bool, String> {
    let mut exit = 0;
    if unsafe { GetExitCodeProcess(handle.0, &mut exit) } == 0 {
        return Err(browser_unknown());
    }
    Ok(exit != STILL_ACTIVE as u32)
}
fn browser_process(
    pid: u32,
    query: &Handle,
    current: &[u8],
    profile: &str,
    close: bool,
) -> Result<bool, String> {
    let result = (|| {
        if process_exited(query)? {
            return Ok(false);
        }
        let user = unsafe { process_sid(query.0) }.ok_or_else(browser_unknown)?;
        if user.as_slice() != current {
            return Ok(false);
        }
        let image = query.image().ok_or_else(browser_unknown)?;
        let companion = companion_image(&image);
        let created = query.created().ok_or_else(browser_unknown)?;
        let args = match browser_arguments::read(query.0) {
            Some(args) => args,
            None if !companion => return Ok(false),
            None => return Err(browser_unknown()),
        };
        if query.created() != Some(created) || query.image().as_deref() != Some(image.as_str()) {
            return Err(browser_unknown());
        }
        // 子渲染器没有独立资料所有权；只由主进程接受正常窗口关闭。
        if args
            .iter()
            .skip(1)
            .take_while(|arg| arg.as_str() != "--")
            .any(|arg| {
                arg.strip_prefix("--type=")
                    .is_some_and(|value| !value.is_empty())
            })
        {
            return Ok(false);
        }
        // 普通 Chrome 默认资料不是本应用资料；显式但畸形的资料参数仍不能冒充未运行。
        if !companion
            && !args
                .iter()
                .skip(1)
                .take_while(|arg| arg.as_str() != "--")
                .any(|arg| arg == "--user-data-dir" || arg.starts_with("--user-data-dir="))
        {
            return Ok(false);
        }
        if !profile_matches(&args, profile)? {
            return Ok(false);
        }
        if close && unsafe { EnumWindows(Some(close_window), pid as LPARAM) } == 0 {
            return Err(browser_unknown());
        }
        Ok(true)
    })();
    // 正常退出可能恰好发生于 argv 读取之间；只有持有原进程句柄证明退出才降级为未运行。
    if result.is_err() && process_exited(query) == Ok(true) {
        Ok(false)
    } else {
        result
    }
}
fn browser(profile: &str, close: bool) -> Result<bool, String> {
    if !Path::new(profile).is_absolute() {
        return Err("资料路径无效。".into());
    }
    match std::fs::metadata(profile) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Ok(metadata) if metadata.is_dir() => {}
        _ => return Err(browser_unknown()),
    }
    let current = current_sid().ok_or_else(browser_unknown)?;
    // 浏览器单独枚举；不改变 Claude CLI/Desktop 既有的精确识别与结束重试语义。
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(browser_unknown());
    }
    let snapshot = Handle(raw);
    let mut entry = unsafe { std::mem::zeroed::<PROCESSENTRY32W>() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut has = unsafe { Process32FirstW(snapshot.0, &mut entry) };
    let mut last_error = if has == 0 {
        unsafe { GetLastError() }
    } else {
        0
    };
    let mut found = false;
    while has != 0 {
        let name = String::from_utf16_lossy(&entry.szExeFile)
            .trim_end_matches('\0')
            .to_lowercase();
        if name == "chrome.exe" && entry.th32ProcessID > 1 {
            let pid = entry.th32ProcessID;
            let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
            if raw.is_null() {
                if unsafe { GetLastError() } != ERROR_INVALID_PARAMETER {
                    return Err(browser_unknown());
                }
            } else {
                let query = Handle(raw);
                found |= browser_process(pid, &query, current, profile, close)?;
            }
        }
        has = unsafe { Process32NextW(snapshot.0, &mut entry) };
        if has == 0 {
            last_error = unsafe { GetLastError() };
        }
    }
    if last_error != ERROR_NO_MORE_FILES {
        return Err(browser_unknown());
    }
    Ok(found)
}
pub unsafe fn rf_browser_running(profile: *const c_char) -> *mut c_char {
    response(browser(input(profile), false).map(|running| json!({"running":running})))
}
pub unsafe fn rf_browser_quit(profile: *const c_char) -> *mut c_char {
    response(browser(input(profile), true).map(|_| json!({"ok":true})))
}
// ----- 本次安装子进程树：超时和宿主退出都不会留下后台安装器 -----
pub(crate) struct ChildJob(Handle);
impl ChildJob {
    pub(crate) fn attach(child: &std::process::Child) -> Result<Self, String> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw.is_null() {
            return Err("无法隔离 Windows 安装子进程。".into());
        }
        let job = Self(Handle(raw));
        let mut limit = unsafe { std::mem::zeroed::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() };
        limit.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                raw,
                JobObjectExtendedLimitInformation,
                (&limit as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limit) as u32,
            )
        } == 0
            || unsafe { AssignProcessToJobObject(raw, child.as_raw_handle()) } == 0
        {
            return Err("无法隔离 Windows 安装子进程。".into());
        }
        Ok(job)
    }
    pub(crate) fn stop(&self) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0 .0, 1);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_arguments_reject_substrings_and_unrelated_scripts() {
        assert!(official_cli(&[
            "node.exe".into(),
            "C:\\test\\node_modules\\@anthropic-ai\\claude-code\\cli.js".into()
        ]));
        assert!(!official_cli(&[
            "node.exe".into(),
            "C:\\other.js".into(),
            "C:\\test\\node_modules\\@anthropic-ai\\claude-code\\cli.js".into()
        ]));
        assert_eq!(
            profile_argument(&[
                "chrome.exe".into(),
                "--user-data-dir=C:\\空 格\\profile".into()
            ]),
            Some(Path::new("C:\\空 格\\profile"))
        );
        let home = std::env::var("USERPROFILE").unwrap();
        let local = std::env::var("LOCALAPPDATA").unwrap();
        assert_eq!(
            claude_kind(&format!(r"{home}\.bun\bin\claude.exe"), &[]),
            Some("Claude Code")
        );
        assert_eq!(
            claude_kind(
                &format!(r"{local}\AnthropicClaude\app\resources\cowork-svc.exe"),
                &[]
            ),
            Some("Claude Desktop")
        );
        for unrelated in [
            format!(r"{home}\.claude\claude-code-helper.exe"),
            format!(r"{home}\.local\share\claude\claude-code-helper.exe"),
            format!(r"{local}\AnthropicClaudeOther\resources\cowork-svc.exe"),
            format!(r"{local}\Programs\Unrelated\cowork-svc.exe"),
            format!(r"{home}\scoop\shims\claude.exe"),
            format!(r"{home}\.volta\bin\claude.exe"),
        ] {
            assert_eq!(claude_kind(&unrelated, &[]), None, "{unrelated}");
        }
        assert!(current_sid().is_some());
        assert!(Handle::open(std::process::id(), false).is_some());
    }

    #[test]
    fn browser_profile_arguments_and_directory_aliases_are_verified() {
        let root = std::env::temp_dir().join(format!(
            "ccbaga-profile-{}",
            crate::identities::new_id().unwrap()
        ));
        let profile = root.join("long profile name");
        let other = root.join("other");
        for path in [&profile, &other] {
            crate::identities::private::directory(path).unwrap();
        }
        let argument = |path: &Path| format!("--user-data-dir={}", path.display());
        let args = vec!["chrome.exe".into(), argument(&profile)];
        assert_eq!(profile_matches(&args, profile.to_str().unwrap()), Ok(true));
        assert_eq!(profile_matches(&args, other.to_str().unwrap()), Ok(false));
        assert_eq!(
            profile_matches(
                &[
                    "chrome.exe".into(),
                    "--user-data-dir".into(),
                    profile.to_string_lossy().into()
                ],
                profile.to_str().unwrap()
            ),
            Ok(true)
        );
        let verbatim = std::fs::canonicalize(&profile).unwrap();
        assert_eq!(
            file_identity(&verbatim, true),
            file_identity(&profile, true)
        );
        let with_dot = profile.join(".");
        assert_eq!(
            file_identity(&with_dot, true),
            file_identity(&profile, true)
        );
        let mut short = [0u16; 32768];
        let length = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetShortPathNameW(
                super::super::wide_path(&profile).as_ptr(),
                short.as_mut_ptr(),
                short.len() as u32,
            )
        };
        assert!(length > 0 && length < short.len() as u32);
        let short =
            std::path::PathBuf::from(String::from_utf16(&short[..length as usize]).unwrap());
        assert_eq!(file_identity(&short, true), file_identity(&profile, true));
        for invalid in [
            vec!["chrome.exe".into()],
            vec!["chrome.exe".into(), "--user-data-dir".into()],
            vec!["chrome.exe".into(), "--user-data-dir=".into()],
            vec!["chrome.exe".into(), "--user-data-dir=relative".into()],
            vec!["chrome.exe".into(), "--user-data-dir=C:relative".into()],
            vec!["chrome.exe".into(), "--user-data-dir=\\relative".into()],
            vec!["chrome.exe".into(), argument(&profile), argument(&profile)],
            vec![
                "chrome.exe".into(),
                argument(&other),
                "--user-data-dir".into(),
                profile.to_string_lossy().into(),
            ],
            vec!["chrome.exe".into(), "--".into(), argument(&profile)],
        ] {
            assert!(profile_matches(&invalid, profile.to_str().unwrap()).is_err());
        }
        assert_eq!(
            profile_matches(
                &[
                    "chrome.exe".into(),
                    argument(&other),
                    "--".into(),
                    argument(&profile)
                ],
                profile.to_str().unwrap()
            ),
            Ok(false)
        );
        assert!(profile_matches(
            &["chrome.exe".into(), argument(&root.join("missing"))],
            profile.to_str().unwrap()
        )
        .is_err());
        assert_eq!(
            browser(root.join("missing").to_str().unwrap(), false),
            Ok(false)
        );
        let file = root.join("file");
        std::fs::write(&file, b"fixture").unwrap();
        assert!(file_identity(&file, true).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
