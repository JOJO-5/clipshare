use std::ffi::c_void;

type Handle = *mut c_void;
type LoadLibraryEx = unsafe extern "system" fn(*const u16, Handle, u32) -> Handle;

const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x0800;
const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x0008;
const ERROR_INVALID_PARAMETER: u32 = 87;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryExW(name: *const u16, file: Handle, flags: u32) -> Handle;
    fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
    fn GetLastError() -> u32;
    fn SetLastError(error: u32);
}

#[no_mangle]
static ClipShare_LoadSysDll: LoadLibraryEx = load_library_ex;

unsafe extern "system" fn load_library_ex(name: *const u16, file: Handle, flags: u32) -> Handle {
    let module = LoadLibraryExW(name, file, flags);
    if !module.is_null()
        || flags != LOAD_LIBRARY_SEARCH_SYSTEM32
        || !file.is_null()
        || name.is_null()
    {
        return module;
    }

    let error = GetLastError();
    if error != ERROR_INVALID_PARAMETER {
        return module;
    }

    // These are the system libraries looked up by the pinned SDK loader. Keep
    // the original search restriction by resolving the DLL under System32.
    let mut system_path = [0u16; 260];
    for dll in ["version.dll", "shell32.dll"] {
        let encoded = dll.encode_utf16().chain([0]);
        if !encoded
            .clone()
            .enumerate()
            .all(|(i, ch)| *name.add(i) == ch)
        {
            continue;
        }
        let length =
            GetSystemDirectoryW(system_path.as_mut_ptr(), system_path.len() as u32) as usize;
        if length == 0 || length + dll.len() + 2 > system_path.len() {
            break;
        }
        system_path[length] = b'\\' as u16;
        for (character, slot) in encoded.zip(&mut system_path[length + 1..]) {
            *slot = character;
        }
        return LoadLibraryExW(system_path.as_ptr(), file, LOAD_WITH_ALTERED_SEARCH_PATH);
    }
    SetLastError(error);
    module
}
