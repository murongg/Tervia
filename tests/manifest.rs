#![cfg(all(windows, feature = "native"))]

use std::{os::windows::ffi::OsStrExt, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    System::LibraryLoader::{
        EnumResourceLanguagesW, EnumResourceNamesW, FindResourceW, LOAD_LIBRARY_AS_DATAFILE,
        LOAD_LIBRARY_AS_IMAGE_RESOURCE, LoadLibraryExW, LoadResource, LockResource, SizeofResource,
    },
};

struct ResourceModule(HMODULE);

impl ResourceModule {
    fn load(path: &Path) -> Self {
        let path: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // Loading only resources does not execute the GUI or require a desktop session in CI.
        let module = unsafe {
            LoadLibraryExW(
                path.as_ptr(),
                ptr::null_mut(),
                LOAD_LIBRARY_AS_DATAFILE | LOAD_LIBRARY_AS_IMAGE_RESOURCE,
            )
        };
        assert!(!module.is_null(), "{}", std::io::Error::last_os_error());
        Self(module)
    }

    fn read(&self, kind: usize, id: usize) -> Vec<u8> {
        // Win32 encodes integer resource IDs as pointer values, rather than string addresses.
        let resource = unsafe {
            FindResourceW(
                self.0,
                ptr::without_provenance(id),
                ptr::without_provenance(kind),
            )
        };
        assert!(!resource.is_null(), "Missing resource {kind}/{id}");
        let size = unsafe { SizeofResource(self.0, resource) };
        assert!(size > 0);
        let loaded = unsafe { LoadResource(self.0, resource) };
        assert!(!loaded.is_null());
        let bytes = unsafe { LockResource(loaded) };
        assert!(!bytes.is_null());
        // Copy before ResourceModule releases the mapped image.
        unsafe { std::slice::from_raw_parts(bytes.cast::<u8>(), size as usize) }.to_vec()
    }
}

impl Drop for ResourceModule {
    fn drop(&mut self) {
        unsafe { FreeLibrary(self.0) };
    }
}

unsafe extern "system" fn collect_names(
    _module: HMODULE,
    _kind: *const u16,
    name: *const u16,
    context: isize,
) -> i32 {
    // EnumResourceNamesW invokes this callback synchronously while the vector is borrowed.
    unsafe { &mut *(context as *mut Vec<usize>) }.push(name as usize);
    1
}

unsafe extern "system" fn collect_languages(
    _module: HMODULE,
    _kind: *const u16,
    _name: *const u16,
    language: u16,
    context: isize,
) -> i32 {
    unsafe { &mut *(context as *mut Vec<u16>) }.push(language);
    1
}

#[test]
fn native_executable_contains_one_manifest_with_all_platform_settings_and_icon() {
    let module = ResourceModule::load(Path::new(env!("CARGO_BIN_EXE_tervia")));
    let mut names = Vec::<usize>::new();
    let enumerated = unsafe {
        EnumResourceNamesW(
            module.0,
            ptr::without_provenance(24),
            Some(collect_names),
            (&mut names as *mut Vec<usize>) as isize,
        )
    };
    assert_ne!(enumerated, 0);
    assert_eq!(
        names,
        [1],
        "The executable must own a single application manifest"
    );

    let mut languages = Vec::<u16>::new();
    let enumerated = unsafe {
        EnumResourceLanguagesW(
            module.0,
            ptr::without_provenance(24),
            ptr::without_provenance(1),
            Some(collect_languages),
            (&mut languages as *mut Vec<u16>) as isize,
        )
    };
    assert_ne!(enumerated, 0);
    assert_eq!(
        languages.len(),
        1,
        "Locale-specific manifests must not override application settings"
    );

    let manifest = String::from_utf8(module.read(24, 1)).unwrap();
    for setting in [
        "Microsoft.Windows.Common-Controls",
        "version=\"6.0.0.0\"",
        "asInvoker",
        "true/pm",
        "PerMonitorV2",
        "SegmentHeap",
        "longPathAware",
        ">true</longPathAware>",
        "activeCodePage",
        "UTF-8",
    ] {
        assert!(
            manifest.contains(setting),
            "Missing embedded manifest setting: {setting}"
        );
    }
    assert!(
        !module.read(14, 1).is_empty(),
        "The app icon group must remain embedded"
    );
}
