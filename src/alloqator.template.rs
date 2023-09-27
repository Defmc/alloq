unsafe impl core::alloc::Allocator for Alloq {
    fn allocate(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
        Alloqator::allocate(self, layout)
    }

    unsafe fn deallocate(&self, ptr: core::ptr::NonNull<u8>, layout: core::alloc::Layout) {
        Alloqator::deallocate(self, ptr, layout)
    }
}

unsafe impl core::alloc::GlobalAlloc for Alloq {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        Alloqator::alloc(self, layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        Alloqator::dealloc(self, ptr, layout)
    }
}
