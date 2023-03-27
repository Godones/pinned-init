// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module contains library-internal items.
//!
//! These items must not be used outside of
//! - `lib.rs`
//! - `../pinned-init-macro/src/pin_data.rs`
//! - `../pinned-init-macro/src/pinned_drop.rs`

use super::*;

/// This trait is only implemented via the `#[pin_data]` proc-macro. It is used to facilitate
/// the pin projections within the initializers.
///
/// # Safety
///
/// Only the `init` module is allowed to use this trait.
pub unsafe trait HasPinData {
    type PinData: PinData;

    unsafe fn __pin_data() -> Self::PinData;
}

/// Marker trait for pinning data of structs.
///
/// # Safety
///
/// Only the `init` module is allowed to use this trait.
pub unsafe trait PinData: Copy {
    type Datee: ?Sized + HasPinData;

    /// Type inference helper function.
    fn make_closure<F, O, E>(self, f: F) -> F
    where
        F: FnOnce(*mut Self::Datee) -> Result<O, E>,
    {
        f
    }
}

/// This trait is automatically implemented for every type. It aims to provide the same type
/// inference help as `HasPinData`.
///
/// # Safety
///
/// Only the `init` module is allowed to use this trait.
pub unsafe trait HasInitData {
    type InitData: InitData;

    unsafe fn __init_data() -> Self::InitData;
}

/// Same function as `PinData`, but for arbitrary data.
///
/// # Safety
///
/// Only the `init` module is allowed to use this trait.
pub unsafe trait InitData: Copy {
    type Datee: ?Sized + HasInitData;

    /// Type inference helper function.
    fn make_closure<F, O, E>(self, f: F) -> F
    where
        F: FnOnce(*mut Self::Datee) -> Result<O, E>,
    {
        f
    }
}

pub struct AllData<T: ?Sized>(PhantomData<fn(Box<T>) -> Box<T>>);

impl<T: ?Sized> Clone for AllData<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for AllData<T> {}

unsafe impl<T: ?Sized> InitData for AllData<T> {
    type Datee = T;
}

unsafe impl<T: ?Sized> HasInitData for T {
    type InitData = AllData<T>;

    unsafe fn __init_data() -> Self::InitData {
        AllData(PhantomData)
    }
}

/// Stack initializer helper type. Use [`stack_pin_init`] instead of this primitive.
///
/// # Invariants
///
/// If `self.1` is true, then `self.0` is initialized.
///
/// [`stack_pin_init`]: pinned_init::stack_pin_init
pub struct StackInit<T>(MaybeUninit<T>, bool);

impl<T> Drop for StackInit<T> {
    #[inline]
    fn drop(&mut self) {
        if self.1 {
            // SAFETY: As we are being dropped, we only call this once. And since `self.1 == true`,
            // `self.0` has to be initialized.
            unsafe { self.0.assume_init_drop() };
        }
    }
}

impl<T> StackInit<T> {
    /// Creates a new [`StackInit<T>`] that is uninitialized. Use [`stack_pin_init`] instead of this
    /// primitive.
    ///
    /// [`stack_pin_init`]: pinned_init::stack_pin_init
    #[inline]
    pub fn uninit() -> Self {
        Self(MaybeUninit::uninit(), false)
    }

    /// Initializes the contents and returns the result.
    ///
    /// # Safety
    ///
    /// The caller ensures that `self` is on the stack and not accessible in any other way, if this
    /// function returns `Ok`.
    #[inline]
    pub unsafe fn init<E>(&mut self, init: impl PinInit<T, E>) -> Result<Pin<&mut T>, E> {
        // SAFETY: The memory slot is valid and this type ensures that it will stay pinned.
        unsafe { init.__pinned_init(self.0.as_mut_ptr())? };
        self.1 = true;
        // SAFETY: The slot is now pinned, since we will never give access to `&mut T`.
        Ok(unsafe { Pin::new_unchecked(self.0.assume_init_mut()) })
    }
}

/// When a value of this type is dropped, it drops a `T`.
///
/// Can be forgotton to prevent the drop.
pub struct DropGuard<T: ?Sized>(*mut T, Cell<bool>);

impl<T: ?Sized> DropGuard<T> {
    /// Creates a new [`DropGuard<T>`]. It will [`ptr::drop_in_place`] `ptr` when it gets dropped.
    ///
    /// # Safety
    ///
    /// `ptr` must be a valid pointer.
    ///
    /// It is the callers responsibility that `self` will only get dropped if the pointee of `ptr`:
    /// - has not been dropped,
    /// - is not accessible by any other means,
    /// - will not be dropped by any other means.
    #[inline]
    pub unsafe fn new(ptr: *mut T) -> Self {
        Self(ptr, Cell::new(true))
    }

    /// Prevents this guard from dropping the supplied pointer.
    ///
    /// # Safety
    ///
    /// This function is unsafe in order to prevent safe code from forgetting this guard. It should
    /// only be called by the macros in this module.
    #[inline]
    pub unsafe fn forget(&self) {
        self.1.set(false);
    }
}

impl<T: ?Sized> Drop for DropGuard<T> {
    #[inline]
    fn drop(&mut self) {
        if self.1.get() {
            // SAFETY: A `DropGuard` can only be constructed using the unsafe `new` function
            // ensuring that this operation is safe.
            unsafe { ptr::drop_in_place(self.0) }
        }
    }
}

/// Token used by `PinnedDrop` to prevent calling the function without creating this unsafely
/// created struct. This is needed, because the `drop` function is safe, but should not be called
/// manually.
pub struct OnlyCallFromDrop(());

impl OnlyCallFromDrop {
    /// # Safety
    ///
    /// This function should only be called from the [`Drop::drop`] function and only be used to
    /// delegate the destruction to the pinned destructor [`PinnedDrop::drop`] of the same type.
    pub unsafe fn create() -> Self {
        Self(())
    }
}
