#![feature(allocator_api)]
#![feature(pointer_is_aligned)]

pub mod list;

//#[cfg(feature = "bump")]
pub mod bump;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AlloqMetaData {
    size: usize,
}

impl AlloqMetaData {
    pub fn new(size: usize) -> Self {
        Self { size }
    }

    pub unsafe fn write_meta(&self, ptr: *mut u8) -> *mut u8 {
        println!(
            "{:?} -> {} .:. {}",
            ptr,
            ptr as usize,
            ptr as usize % core::mem::size_of::<Self>()
        );
        *(ptr as *mut AlloqMetaData) = *self;
        ptr.offset(core::mem::size_of::<AlloqMetaData>() as isize)
    }

    pub const fn total_size(&self) -> usize {
        self.size + core::mem::size_of::<AlloqMetaData>()
    }

    pub unsafe fn from_ptr(ptr: *const u8) -> AlloqMetaData {
        *(ptr as *const AlloqMetaData)
    }
}
