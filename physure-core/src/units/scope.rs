//! A unit catalog for the duration of a call.
//!
//! Unit text (`km`, `5 fur`, a `to("ft")`) is read against a catalog. Code that has no registry in hand
//! (`Quantity::new`, the PHS parser deciding whether a word after a number is a unit) reads it against the
//! embedded one. [`with_catalog`] says "for everything this call does on this thread, read unit text
//! against this registry instead": a binding that owns a registry built from a custom conf wraps the
//! PHS evaluation it runs for that registry, and `1 km` on a registry that defines `km = 7.0` is 7 m, as
//! `getUnitScale("km")` already says.
//!
//! The scope is a thread-local stack, so it is not a global: another thread does not see it, a scope
//! inside a scope wins and gives way to the outer one when it ends (also when it ends in a panic), and
//! code that never sets one reads what it always read. It must not be held across a thread switch; the
//! closure given to [`with_catalog`] runs to the end on the thread that called it.

use crate::units::registry::UnitRegistry;
use std::cell::RefCell;

thread_local! {
    // Pointers, not owners: copying a catalog into the stack would cost what the scope exists to avoid. A
    // pointer is pushed by `with_catalog`, which borrows the registry until it pops it, and read only
    // while it is on the stack.
    static CATALOGS: RefCell<Vec<*const UnitRegistry>> = const { RefCell::new(Vec::new()) };
}

/// Pops the scope it was made for, however `with_catalog` ends.
struct Pop;

impl Drop for Pop {
    fn drop(&mut self) {
        CATALOGS.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

/// Runs `f` with `registry` as the catalog that unit text is read against, on this thread, until it
/// returns. Scopes nest: the innermost one wins and the outer one is back when it ends.
pub fn with_catalog<R>(registry: &UnitRegistry, f: impl FnOnce() -> R) -> R {
    CATALOGS.with(|stack| stack.borrow_mut().push(registry as *const UnitRegistry));
    let _pop = Pop;
    f()
}

/// Runs `f` with the catalog of the innermost [`with_catalog`] on this thread, or `None` when there is no
/// scope (read against the embedded catalog, as before).
pub fn with_current_catalog<R>(f: impl FnOnce(Option<&UnitRegistry>) -> R) -> R {
    let top = CATALOGS.with(|stack| stack.borrow().last().copied());
    // SAFETY: a pointer is on the stack only between the `push` in `with_catalog` and the `Pop` that
    // removes it, and `with_catalog` holds the `&UnitRegistry` it was given for that whole time, so the
    // registry is alive. The stack is thread-local, so this is the thread that holds the borrow.
    f(top.map(|pointer| unsafe { &*pointer }))
}
