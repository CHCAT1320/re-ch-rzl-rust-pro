use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use jni::JNIEnv;
use jni::objects::{GlobalRef, JObject, JString};
use jni::sys::{jint, jobject, jstring};
use jni::JavaVM;
use macroquad::prelude::*;

static VM: Mutex<Option<JavaVM>> = Mutex::new(None);
static ACTIVITY: Mutex<Option<GlobalRef>> = Mutex::new(None);
static PICKED: Mutex<Option<PathBuf>> = Mutex::new(None);
static OPEN: AtomicBool = AtomicBool::new(false);
static PLAY_CHOICE: AtomicI32 = AtomicI32::new(-1);

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_chcat1320_rechrzl_MainActivity_nativeSetActivity(
    env: *mut jni::sys::JNIEnv,
    _this: jobject,
    activity: jobject,
) {
    let Ok(env) = (unsafe { JNIEnv::from_raw(env) }) else {
        return;
    };
    let Ok(vm) = env.get_java_vm() else {
        return;
    };
    let activity = unsafe { JObject::from_raw(activity) };
    let Ok(global) = env.new_global_ref(&activity) else {
        std::mem::forget(activity);
        return;
    };
    std::mem::forget(activity);
    *VM.lock().unwrap() = Some(vm);
    *ACTIVITY.lock().unwrap() = Some(global);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_chcat1320_rechrzl_MainActivity_nativePickResult(
    env: *mut jni::sys::JNIEnv,
    _this: jobject,
    path: jstring,
) {
    OPEN.store(false, Ordering::SeqCst);
    let Ok(mut env) = (unsafe { JNIEnv::from_raw(env) }) else {
        return;
    };
    if path.is_null() {
        *PICKED.lock().unwrap() = None;
        return;
    }
    let path = unsafe { JString::from_raw(path) };
    let Ok(text) = env.get_string(&path) else {
        *PICKED.lock().unwrap() = None;
        return;
    };
    let value = text.to_string_lossy();
    *PICKED.lock().unwrap() = if value.is_empty() {
        None
    } else {
        Some(PathBuf::from(value.as_ref()))
    };
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_chcat1320_rechrzl_MainActivity_nativePlayChoice(
    _env: *mut jni::sys::JNIEnv,
    _this: jobject,
    play: jint,
) {
    PLAY_CHOICE.store(play, Ordering::SeqCst);
}

pub fn is_open() -> bool {
    OPEN.load(Ordering::SeqCst)
}

pub fn take_picked() -> Option<PathBuf> {
    PICKED.lock().unwrap().take()
}

pub async fn ask_play() -> bool {
    PLAY_CHOICE.store(-1, Ordering::SeqCst);
    for _ in 0..60 {
        if with_env_activity(|env, activity| {
            env.call_method(activity, "rustAskPlay", "()V", &[]).ok()?;
            Some(())
        })
        .is_some() {
            break;
        }
        next_frame().await;
    }
    loop {
        let choice = PLAY_CHOICE.load(Ordering::SeqCst);
        if choice >= 0 {
            return choice != 0;
        }
        next_frame().await;
    }
}

pub async fn pick_file(mime: &str, prompt: &str) -> PathBuf {
    loop {
        if let Some(path) = take_picked() {
            return path;
        }
        if !is_open() {
            open_picker(mime);
        }
        clear_background(BLACK);
        draw_text(prompt, 30.0, 80.0, 28.0, WHITE);
        next_frame().await;
    }
}

pub fn show_result(text: &str) {
    let _ = with_env_activity(|env, activity| {
        let java_text = env.new_string(text).ok()?;
        env.call_method(
            activity,
            "rustShowResult",
            "(Ljava/lang/String;)V",
            &[jni::objects::JValueGen::Object(&java_text)],
        )
        .ok()?;
        Some(())
    });
}

fn open_picker(mime: &str) {
    OPEN.store(true, Ordering::SeqCst);
    *PICKED.lock().unwrap() = None;
    let launched = with_env_activity(|env, activity| {
        let java_mime = env.new_string(mime).ok()?;
        env.call_method(
            activity,
            "rustOpenPicker",
            "(Ljava/lang/String;)V",
            &[jni::objects::JValueGen::Object(&java_mime)],
        )
        .ok()?;
        Some(())
    })
    .is_some();
    if !launched {
        OPEN.store(false, Ordering::SeqCst);
    }
}

fn with_env_activity<T>(f: impl FnOnce(&mut JNIEnv, &JObject) -> Option<T>) -> Option<T> {
    let vm = {
        let guard = VM.lock().ok()?;
        let ptr = guard.as_ref()?.get_java_vm_pointer();
        unsafe { JavaVM::from_raw(ptr).ok()? }
    };
    let activity = ACTIVITY.lock().ok()?.as_ref()?.clone();
    let mut env = vm.attach_current_thread().ok()?;
    f(&mut env, activity.as_obj())
}
