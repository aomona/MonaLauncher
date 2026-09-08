use std::ffi::{c_char, c_void, CStr};
use std::ptr;
use std::sync::atomic::{AtomicI32, AtomicPtr, Ordering};

type JInt = i32;
type JLong = i64;
type JObject = *mut c_void;
type JClass = JObject;
type JThread = JObject;
type JMethodId = *mut c_void;
type FunctionTable = *const *const c_void;
type JavaVm = FunctionTable;
type JavaVmPtr = *mut JavaVm;
type JvmtiEnv = FunctionTable;
type JvmtiEnvPtr = *mut JvmtiEnv;

const JNI_OK: JInt = 0;
const JVMTI_VERSION_1_2: JInt = 0x3001_0200;
const JVMTI_ENABLE: JInt = 1;
const JVMTI_EVENT_CLASS_PREPARE: JInt = 56;
const JVMTI_EVENT_BREAKPOINT: JInt = 62;
const GLFW_CURSOR: JInt = 0x0003_3001;
const GLFW_CURSOR_NORMAL: JInt = 0x0003_4001;
const GLFW_CURSOR_DISABLED: JInt = 0x0003_4003;
const GLFW_CURSOR_CAPTURED: JInt = 0x0003_4004;
const STD_OUTPUT_HANDLE: i32 = -11;

static GLFW_CURSOR_METHOD: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static LWJGL2_CURSOR_METHOD: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static LAST_STATE: AtomicI32 = AtomicI32::new(-1);
static mut TOKEN: [u8; 64] = [0; 64];

#[repr(C)]
struct JvmtiCapabilities {
    first_bits: u32,
    reserved: [u16; 6],
}

#[repr(C)]
struct JvmtiEventCallbacks {
    callbacks: [*const c_void; 39],
}

#[link(name = "kernel32")]
extern "system" {
    fn GetStdHandle(kind: i32) -> *mut c_void;
    fn WriteFile(
        file: *mut c_void,
        buffer: *const c_void,
        bytes: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
}

unsafe fn function(env: JvmtiEnvPtr, number: usize) -> *const c_void {
    *(*env).add(number - 1)
}

unsafe extern "system" fn class_prepare(
    env: JvmtiEnvPtr,
    _jni: *mut c_void,
    _thread: JThread,
    class: JClass,
) {
    type GetClassSignature =
        unsafe extern "system" fn(JvmtiEnvPtr, JClass, *mut *mut c_char, *mut *mut c_char) -> JInt;
    type GetClassMethods =
        unsafe extern "system" fn(JvmtiEnvPtr, JClass, *mut JInt, *mut *mut JMethodId) -> JInt;
    type GetMethodName = unsafe extern "system" fn(
        JvmtiEnvPtr,
        JMethodId,
        *mut *mut c_char,
        *mut *mut c_char,
        *mut *mut c_char,
    ) -> JInt;
    type SetBreakpoint = unsafe extern "system" fn(JvmtiEnvPtr, JMethodId, JLong) -> JInt;
    type Deallocate = unsafe extern "system" fn(JvmtiEnvPtr, *mut u8) -> JInt;

    let get_signature: GetClassSignature = std::mem::transmute(function(env, 48));
    let deallocate: Deallocate = std::mem::transmute(function(env, 47));
    let mut signature = ptr::null_mut();
    if get_signature(env, class, &mut signature, ptr::null_mut()) != 0 || signature.is_null() {
        return;
    }
    let signature_bytes = CStr::from_ptr(signature).to_bytes();
    let is_glfw = signature_bytes == b"Lorg/lwjgl/glfw/GLFW;";
    let is_lwjgl2_mouse = signature_bytes == b"Lorg/lwjgl/input/Mouse;";
    deallocate(env, signature.cast());
    if !is_glfw && !is_lwjgl2_mouse {
        return;
    }

    let get_methods: GetClassMethods = std::mem::transmute(function(env, 52));
    let get_name: GetMethodName = std::mem::transmute(function(env, 64));
    let set_breakpoint: SetBreakpoint = std::mem::transmute(function(env, 38));
    let mut count = 0;
    let mut methods = ptr::null_mut();
    if get_methods(env, class, &mut count, &mut methods) != 0 || methods.is_null() {
        return;
    }
    for index in 0..count.max(0) as usize {
        let method = *methods.add(index);
        let mut name = ptr::null_mut();
        let mut descriptor = ptr::null_mut();
        let is_cursor_method = get_name(env, method, &mut name, &mut descriptor, ptr::null_mut())
            == 0
            && !name.is_null()
            && !descriptor.is_null()
            && ((is_glfw
                && CStr::from_ptr(name).to_bytes() == b"glfwSetInputMode"
                && CStr::from_ptr(descriptor).to_bytes() == b"(JII)V")
                || (is_lwjgl2_mouse
                    && CStr::from_ptr(name).to_bytes() == b"setGrabbed"
                    && CStr::from_ptr(descriptor).to_bytes() == b"(Z)V"));
        if is_cursor_method && set_breakpoint(env, method, 0) == 0 {
            if is_glfw {
                GLFW_CURSOR_METHOD.store(method, Ordering::Release);
            } else {
                LWJGL2_CURSOR_METHOD.store(method, Ordering::Release);
            }
        }
        if !name.is_null() {
            deallocate(env, name.cast());
        }
        if !descriptor.is_null() {
            deallocate(env, descriptor.cast());
        }
    }
    deallocate(env, methods.cast());
}

unsafe extern "system" fn breakpoint(
    env: JvmtiEnvPtr,
    _jni: *mut c_void,
    thread: JThread,
    method: JMethodId,
    _location: JLong,
) {
    let is_glfw = GLFW_CURSOR_METHOD.load(Ordering::Acquire) == method;
    let is_lwjgl2_mouse = LWJGL2_CURSOR_METHOD.load(Ordering::Acquire) == method;
    if !is_glfw && !is_lwjgl2_mouse {
        return;
    }
    type GetLocalInt =
        unsafe extern "system" fn(JvmtiEnvPtr, JThread, JInt, JInt, *mut JInt) -> JInt;
    let get_local: GetLocalInt = std::mem::transmute(function(env, 22));
    let state = if is_glfw {
        let mut mode = 0;
        let mut value = GLFW_CURSOR_NORMAL;
        if get_local(env, thread, 0, 2, &mut mode) != 0
            || get_local(env, thread, 0, 3, &mut value) != 0
            || mode != GLFW_CURSOR
        {
            return;
        }
        i32::from(value == GLFW_CURSOR_DISABLED || value == GLFW_CURSOR_CAPTURED)
    } else {
        let mut grabbed = 0;
        if get_local(env, thread, 0, 0, &mut grabbed) != 0 {
            return;
        }
        i32::from(grabbed != 0)
    };
    if LAST_STATE.swap(state, Ordering::AcqRel) != state {
        emit(if state == 1 { b"GRAB" } else { b"RELEASE" });
    }
}

unsafe fn emit(command: &[u8]) {
    const PREFIX: &[u8] = b"MONALAUNCHER_CURSOR\t";
    let mut line = [0_u8; 128];
    let mut length = 0;
    line[..PREFIX.len()].copy_from_slice(PREFIX);
    length += PREFIX.len();
    ptr::copy_nonoverlapping(
        ptr::addr_of!(TOKEN).cast::<u8>(),
        line.as_mut_ptr().add(length),
        64,
    );
    length += 64;
    line[length] = b'\t';
    length += 1;
    line[length..length + command.len()].copy_from_slice(command);
    length += command.len();
    line[length..length + 2].copy_from_slice(b"\r\n");
    length += 2;
    let output = GetStdHandle(STD_OUTPUT_HANDLE);
    if output.is_null() || output as isize == -1 {
        return;
    }
    let mut written = 0;
    let _ = WriteFile(
        output,
        line.as_ptr().cast(),
        length as u32,
        &mut written,
        ptr::null_mut(),
    );
}

fn valid_token(options: *mut c_char) -> bool {
    if options.is_null() {
        return false;
    }
    let bytes = unsafe { CStr::from_ptr(options) }.to_bytes();
    bytes.len() == 64 && bytes.iter().all(|value| value.is_ascii_hexdigit())
}

#[no_mangle]
pub unsafe extern "system" fn Agent_OnLoad(
    vm: JavaVmPtr,
    options: *mut c_char,
    _reserved: *mut c_void,
) -> JInt {
    if vm.is_null() || !valid_token(options) {
        return -1;
    }
    ptr::copy_nonoverlapping(
        CStr::from_ptr(options).to_bytes().as_ptr(),
        ptr::addr_of_mut!(TOKEN).cast::<u8>(),
        64,
    );

    type GetEnv = unsafe extern "system" fn(JavaVmPtr, *mut *mut c_void, JInt) -> JInt;
    let vm_table = *vm;
    let get_env: GetEnv = std::mem::transmute(*vm_table.add(6));
    let mut env: JvmtiEnvPtr = ptr::null_mut();
    if get_env(vm, (&mut env as *mut JvmtiEnvPtr).cast(), JVMTI_VERSION_1_2) != JNI_OK
        || env.is_null()
    {
        return -1;
    }

    type AddCapabilities = unsafe extern "system" fn(JvmtiEnvPtr, *const JvmtiCapabilities) -> JInt;
    type SetCallbacks =
        unsafe extern "system" fn(JvmtiEnvPtr, *const JvmtiEventCallbacks, JInt) -> JInt;
    type SetEvent = unsafe extern "system" fn(JvmtiEnvPtr, JInt, JInt, JThread) -> JInt;
    let add_capabilities: AddCapabilities = std::mem::transmute(function(env, 142));
    let capabilities = JvmtiCapabilities {
        // can_access_local_variables (bit 14) and can_generate_breakpoint_events (bit 19)
        first_bits: (1 << 14) | (1 << 19),
        reserved: [0; 6],
    };
    if add_capabilities(env, &capabilities) != 0 {
        return -1;
    }

    let mut callbacks = JvmtiEventCallbacks {
        callbacks: [ptr::null(); 39],
    };
    callbacks.callbacks[6] = class_prepare as *const c_void;
    callbacks.callbacks[12] = breakpoint as *const c_void;
    let set_callbacks: SetCallbacks = std::mem::transmute(function(env, 122));
    if set_callbacks(
        env,
        &callbacks,
        std::mem::size_of::<JvmtiEventCallbacks>() as JInt,
    ) != 0
    {
        return -1;
    }
    let set_event: SetEvent = std::mem::transmute(function(env, 2));
    if set_event(
        env,
        JVMTI_ENABLE,
        JVMTI_EVENT_CLASS_PREPARE,
        ptr::null_mut(),
    ) != 0
        || set_event(env, JVMTI_ENABLE, JVMTI_EVENT_BREAKPOINT, ptr::null_mut()) != 0
    {
        return -1;
    }
    JNI_OK
}
