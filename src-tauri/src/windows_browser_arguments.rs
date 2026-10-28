// [INPUT]: 已持有的 Windows QUERY_LIMITED_INFORMATION 进程句柄与原生 NtQueryInformationProcess。
// [OUTPUT]: 有界完整 argv；UNICODE_STRING 的本地缓冲范围/长度/对齐或读取未知均拒绝，不要求浏览器沙箱授予 VM_READ。
// [POS]: windows_process 的浏览器参数读取边界；CLI 仍走既有 PEB 路径，参数不进入 UI、日志或网络。
// [PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md
use super::{parsed_arguments, NtQueryInformationProcess, Unicode};
use windows_sys::Win32::Foundation::HANDLE;

// Windows 8.1+ ProcessCommandLineInformation 返回当前调用方缓冲内的 UNICODE_STRING。
const COMMAND_LINE: u32 = 60;
const MAXIMUM: usize = 1024 * 1024;

fn decode(buffer: &[usize]) -> Option<Vec<String>> {
    let start = buffer.as_ptr() as usize;
    let size = std::mem::size_of_val(buffer);
    if size < std::mem::size_of::<Unicode>() {
        return None;
    }
    let command = unsafe { *(buffer.as_ptr().cast::<Unicode>()) };
    let end = start.checked_add(size)?;
    if command.length == 0
        || command.length % 2 != 0
        || command.length > command.maximum
        || command.buffer % std::mem::align_of::<u16>() != 0
        || command.buffer < start.checked_add(std::mem::size_of::<Unicode>())?
        || command.buffer.checked_add(command.length as usize)? > end
    {
        return None;
    }
    let text = unsafe {
        std::slice::from_raw_parts(command.buffer as *const u16, command.length as usize / 2)
    };
    if text.contains(&0) || String::from_utf16(text).is_err() {
        return None;
    }
    parsed_arguments(text)
}

pub(super) fn read(handle: HANDLE) -> Option<Vec<String>> {
    let mut needed = 0;
    unsafe {
        NtQueryInformationProcess(handle, COMMAND_LINE, std::ptr::null_mut(), 0, &mut needed);
    }
    let bytes = needed as usize;
    if !(std::mem::size_of::<Unicode>()..=MAXIMUM).contains(&bytes) {
        return None;
    }
    let mut buffer = vec![0usize; bytes.div_ceil(std::mem::size_of::<usize>())];
    if unsafe {
        NtQueryInformationProcess(
            handle,
            COMMAND_LINE,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } != 0
        || needed as usize > std::mem::size_of_val(buffer.as_slice())
    {
        return None;
    }
    decode(&buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_only_self_handle_reads_complete_arguments_and_rejects_invalid_buffers() {
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, std::process::id()) };
        assert!(!raw.is_null());
        let handle = super::super::Handle(raw);
        assert_eq!(
            read(handle.0).unwrap(),
            std::env::args().collect::<Vec<_>>()
        );
        assert!(read(std::ptr::null_mut()).is_none());
        assert!(decode(&[]).is_none());
        let mut buffer = [0usize; 16];
        let header = buffer.as_mut_ptr().cast::<Unicode>();
        unsafe {
            *header = Unicode {
                length: 2,
                maximum: 2,
                buffer: 2,
            };
            assert!(decode(&buffer).is_none());
            (*header).buffer = buffer.as_ptr() as usize + std::mem::size_of_val(&buffer);
            assert!(decode(&buffer).is_none());
            (*header).buffer = buffer.as_ptr() as usize + std::mem::size_of::<Unicode>();
            (*header).length = 3;
            assert!(decode(&buffer).is_none());
            (*header).length = 2;
            (*header).buffer += 1;
            assert!(decode(&buffer).is_none());
        }
    }
}
