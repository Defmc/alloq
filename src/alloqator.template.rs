unsafe impl core::alloc::GlobalAlloc for Alloq {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        crate::Alloqator::alloq(self, layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        crate::Alloqator::dealloq(self, ptr, layout)
    }
}

unsafe impl Send for Alloq {}
unsafe impl Sync for Alloq {}
