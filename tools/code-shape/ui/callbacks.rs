#![allow(dead_code)]

use std::marker::PhantomData;
use std::ops::Fn as RenamedFn;

type Named<'request> = &'request dyn Fn(u8) -> u8;
type Opaque<'a> = &'a dyn Fn(u8) -> u8;
type Pointer = unsafe extern "C" fn(u8) -> u8;

trait Inspect: Fn(u8) -> u8 {}
impl<Callback> Inspect for Callback
where
    Callback: Fn(u8) -> u8,
{}

fn named(callback: impl Inspect) -> u8 {
    callback(3)
}

fn unnamed(callback: impl RenamedFn(u8) -> u8) -> u8 {
    callback(3)
}

fn generic<Callback>(callback: Callback) -> u8
where
    Callback: Fn(u8) -> u8,
{
    callback(3)
}

struct Handler {
    call: Box<dyn Fn(u8) -> u8>,
    pointer: fn(u8) -> u8,
    marker: PhantomData<fn() -> u8>,
}

#[repr(C)]
struct ForeignOptions {
    #[expect(anonymous_callback, reason = "the foreign header requires an inline nullable function pointer")]
    callback: Option<unsafe extern "C" fn(usize)>,
}

trait Associated {
    type Handler;
}

impl Associated for () {
    type Handler = fn(u8) -> u8;
}

struct Wrapper<Callback>(Callback);
impl<Callback> Associated for Wrapper<Callback>
where
    Callback: Fn(u8) -> u8,
{
    type Handler = Callback;
}

trait Borrowed<'request>: Fn(&'request u8) -> u8 {}
trait Inherited<'a>: Borrowed<'a> {}
type InheritedAlias<'b> = &'b dyn Inherited<'b>;
type NamedPointer = for<'request> fn(&'request u8) -> u8;
type Variance<'a> = PhantomData<fn(&'a u8)>;

fn main() {}

fn exported(_: u8) {}

fn assert_signature() {
    let _: fn(u8) = exported;
}

#[repr(C)]
struct FixedAbi {
    callback: Option<extern "C" fn(u8)>,
}

type NullableCallback = Option<extern "C" fn(u8)>;

#[repr(C)]
struct NamedAbi {
    callback: NullableCallback,
}

trait FutureFactory: FnOnce() -> Self::Future {
    type Future: std::future::Future<Output = u8>;
}

impl<Callback, Future> FutureFactory for Callback
where
    Callback: FnOnce() -> Future + ?Sized,
    Future: std::future::Future<Output = u8>,
{
    type Future = Future;
}

struct FutureWrapper<Callback>(Callback);

impl<Callback, Future> Associated for FutureWrapper<Callback>
where
    Callback: FutureFactory<Future = Future>,
    Future: std::future::Future<Output = u8>,
{
    type Handler = Future;
}

type ThreadBody = Box<dyn FnOnce() + Send + 'static>;

trait ThreadSpawner {
    fn spawn(body: ThreadBody);
}

trait AssociatedCallback {
    type Call<'a>: Fn(&'a u8) -> u8;
}

impl AssociatedCallback for Handler {
    type Call<'b> = &'b dyn Fn(&'b u8) -> u8;
}

impl ThreadSpawner for Handler {
    fn spawn(body: ThreadBody) {
        body();
    }
}

trait Container<Callback>
where
    Callback: Fn(u8) -> u8,
{}
