//! Kernel-safe helpers.
//!
//! `copy_bytes` is an inlined byte-by-byte copy using **volatile** reads and
//! writes. It MUST NOT lower to a `memcpy` libcall: in a kernel module LLVM
//! emits `memcpy` as a GOT-indirect call (`call *memcpy@GOTPCREL(%rip)`)
//! which the module loader cannot relocate, causing a crash. A plain `for`
//! loop is *also* unsafe — LLVM's idiom recognition converts it back into a
//! `memcpy` libcall under `-O` + LTO. Volatile operations cannot be merged,
//! so the copy always stays as byte moves.
//!
//! Use `copy_bytes` anywhere a runtime-sized slice copy is needed in code
//! that runs inside the kernel (firmware scatter, beacon/probe payloads, etc).

/// Inlined volatile byte-by-byte copy. Precondition: `dst.len() >= src.len()`.
#[inline(always)]
pub fn copy_bytes(dst: &mut [u8], src: &[u8]) {
    for i in 0..src.len() {
        // SAFETY: caller guarantees dst.len() >= src.len(); indices in range.
        unsafe {
            core::ptr::write_volatile(
                dst.as_mut_ptr().add(i),
                core::ptr::read_volatile(src.as_ptr().add(i)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn copy_bytes_copies_all() {
        let src = b"hello world";
        let mut dst = [0u8; 32];
        super::copy_bytes(&mut dst[..src.len()], src);
        assert_eq!(&dst[..src.len()], src);
    }

    #[test]
    fn copy_bytes_partial() {
        let src = [1u8, 2, 3, 4, 5];
        let mut dst = [9u8; 8];
        super::copy_bytes(&mut dst[..3], &src[..3]);
        assert_eq!(&dst[..3], &[1, 2, 3]);
        assert_eq!(&dst[3..], &[9, 9, 9, 9, 9]);
    }

    #[test]
    fn copy_bytes_empty() {
        let mut dst = [7u8; 4];
        super::copy_bytes(&mut dst[..0], &[]);
        assert_eq!(dst, [7, 7, 7, 7]);
    }
}
