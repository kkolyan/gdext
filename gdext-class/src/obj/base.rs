use crate::obj::Gd;
use crate::sys;
use crate::traits::GodotClass;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};

/// Smart pointer holding a Godot base class inside a user's `GodotClass`.
///
/// Behaves similarly to [`Gd`][crate::obj::Gd], but is more constrained. Cannot be constructed by the user.
pub struct Base<T: GodotClass> {
    // Internal smart pointer is never dropped. It thus acts like a weak pointer and is needed to break reference cycles between Gd<T>
    // and the user instance owned by InstanceStorage.
    //
    // There is no data apart from the opaque bytes, so no memory or resources to deallocate.
    // When triggered by Godot/GDScript, the destruction order is as follows:
    // 1.    Most-derived Godot class (C++)
    //      ...
    // 2.  RefCounted (C++)
    // 3. Object (C++) -- this triggers InstanceStorage destruction
    // 4.   Base<T>
    // 5.  User struct (GodotClass implementation)
    // 6. InstanceStorage
    //
    // When triggered by Rust (Gd::drop on last strong ref), it's as follows:
    // 1.   Gd<T>  -- triggers InstanceStorage destruction
    // 2.
    obj: ManuallyDrop<Gd<T>>,
}

impl<T: GodotClass> Base<T> {
    // Note: not &mut self, to only borrow one field and not the entire struct
    pub(crate) unsafe fn from_sys(base_ptr: sys::GDNativeObjectPtr) -> Self {
        assert!(!base_ptr.is_null(), "instance base is null pointer");

        let obj = Gd::from_obj_sys(base_ptr);

        // This object does not contribute to the strong count, otherwise we create a reference cycle:
        // 1. RefCounted (dropped in GDScript)
        // 2. holds user T (via extension instance and storage)
        // 3. holds #[base] RefCounted (last ref, dropped in T destructor, but T is never destroyed because this ref keeps storage alive)
        // Note that if late-init never happened on self, we have the same behavior (still a raw pointer instead of weak Gd)
        Base::from_obj(obj)
    }

    fn from_obj(obj: Gd<T>) -> Self {
        Self {
            obj: ManuallyDrop::new(obj),
        }
    }
}

impl<T: GodotClass> std::fmt::Debug for Base<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Base {{ id: {} }}", self.obj.instance_id())
    }
}

impl<T: GodotClass> Deref for Base<T> {
    type Target = Gd<T>;

    fn deref(&self) -> &Self::Target {
        &self.obj
    }
}

// Note: having DerefMut is almost equivalent to directly storing Gd<T>
// Main difference is that an existing Gd<T> cannot be used as the base, and mem::take/replace() don't work as easily
impl<T: GodotClass> DerefMut for Base<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.obj
    }
}
