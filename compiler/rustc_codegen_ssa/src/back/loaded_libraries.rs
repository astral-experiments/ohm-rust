use std::{fs, str};

/// Major and minor device numbers and inode, as reported by the Linux kernel.
pub(super) type FileId = (u32, u32, u64);

/// Identify the actual compiler, backend bridge and LLVM providers. A loader
/// search path can keep the same spelling while resolving to a different DSO.
pub(super) fn cache_provider_ids(
    addresses: [usize; 3],
    sdk_files: &[FileId],
) -> Option<[FileId; 3]> {
    let maps = fs::read("/proc/self/maps").ok()?;
    provider_ids(&maps, addresses, sdk_files)
}

fn provider_ids(maps: &[u8], addresses: [usize; 3], sdk_files: &[FileId]) -> Option<[FileId; 3]> {
    let [Some(driver), Some(backend), Some(llvm)] = addresses.map(|address| {
        maps.split(|&byte| byte == b'\n').find_map(|line| {
            // Parse only the numeric prefix. Mapped filenames can be non-UTF-8,
            // contain spaces, or identify a file that has since been unlinked.
            let mut fields = line.split(u8::is_ascii_whitespace).filter(|word| !word.is_empty());
            let (lo, hi) = str::from_utf8(fields.next()?).ok()?.split_once('-')?;
            let range = usize::from_str_radix(lo, 16).ok()?..usize::from_str_radix(hi, 16).ok()?;
            if !range.contains(&address) {
                return None;
            }
            let (major, minor) = str::from_utf8(fields.nth(2)?).ok()?.split_once(':')?;
            let id = (
                u32::from_str_radix(major, 16).ok()?,
                u32::from_str_radix(minor, 16).ok()?,
                str::from_utf8(fields.next()?).ok()?.parse().ok()?,
            );
            (id.2 != 0 && sdk_files.contains(&id)).then_some(id)
        })
    }) else {
        return None;
    };
    Some([driver, backend, llvm])
}

#[cfg(test)]
mod tests {
    use super::provider_ids;

    #[test]
    fn identifies_mapped_images_even_when_their_names_are_replaced() {
        let maps = b"10-20 r-xp 0 fd:01 10 /sdk/lib/librustc_driver.so\n\
            20-30 r-xp 0 fd:01 20 /sdk/lib/libLLVM.so (deleted)\n\
            30-40 r-xp 0 fd:01 21 /different name\xff\n";
        let driver = (253, 1, 10);
        let old = (253, 1, 20);
        let replacement = (253, 1, 21);
        assert_eq!(
            provider_ids(maps, [0x10, 0x13, 0x2f], &[driver, old]),
            Some([driver, driver, old])
        );
        assert_eq!(provider_ids(maps, [0x10, 0x13, 0x20], &[driver, replacement]), None);
        assert_eq!(
            provider_ids(maps, [0x10, 0x13, 0x30], &[driver, replacement]),
            Some([driver, driver, replacement])
        );
        assert_eq!(provider_ids(maps, [0x10, 0x13, 0x40], &[driver, replacement]), None);
    }
}
