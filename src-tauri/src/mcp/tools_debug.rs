//! macOS-only MCP debug tools: PNG snapshots and JS evaluation with a returned value, for any
//! of the app's webviews (including the embedded admin members view). Both render through
//! WKWebView itself, so neither needs macOS screen-recording permission.

use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
use objc2_foundation::{NSDictionary, NSError, NSString};
use objc2_web_kit::WKWebView;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, Webview};

use super::rpc::Tool;
use super::tools::{object, require_str, string_prop, tool};

/// Generous because the completion handler runs on the main run loop, which a modal or a
/// busy frame can hold up for a while.
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

pub fn all(app: &AppHandle) -> Vec<Tool> {
    vec![screenshot_webview(app), eval_webview(app)]
}

fn webview_prop() -> Value {
    string_prop(
        "Webview label: \"main\" (the Preferences UI) or \"admin\" (the embedded claude.ai members view).",
    )
}

fn resolve(app: &AppHandle, args: &Value) -> Result<Webview, String> {
    let label = require_str(args, "webview")?;
    app.get_webview(&label).ok_or_else(|| {
        let known: Vec<String> = app.webviews().keys().cloned().collect();
        format!("no webview \"{label}\" — known: {known:?}")
    })
}

fn screenshot_webview(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "screenshot_webview",
        "Save a PNG snapshot of one of the app's webviews and return its file path. Rendered by WKWebView itself, so it works without screen-recording permission; the webview must be showing to have pixels.",
        object(json!({"webview": webview_prop()}), &["webview"]),
        move |args| {
            let webview = resolve(&app, args)?;
            let label = webview.label().to_string();
            let (sender, receiver) = mpsc::channel();
            webview
                .with_webview(move |platform| {
                    let block = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
                        let _ = sender.send(unsafe { encode_png(image, error) });
                    });
                    // SAFETY: `inner` is the live WKWebView and `with_webview` runs on the
                    // main thread, where WebKit requires these calls to happen.
                    unsafe {
                        let wk = &*platform.inner().cast::<WKWebView>();
                        wk.takeSnapshotWithConfiguration_completionHandler(None, &block);
                    }
                })
                .map_err(|e| e.to_string())?;
            let bytes = receiver
                .recv_timeout(REPLY_TIMEOUT)
                .map_err(|_| "snapshot timed out".to_string())??;
            let millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!("cdm-{label}-{millis}.png"));
            std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
            Ok(json!({"path": path.display().to_string(), "bytes": bytes.len()}))
        },
    )
}

fn eval_webview(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "eval_webview",
        "Run JavaScript in one of the app's webviews and return its result. `script` is a function body — `return` a JSON-serializable value. Runs from the native side, outside the page's CSP.",
        object(
            json!({
                "webview": webview_prop(),
                "script": string_prop("JavaScript function body. `return` a JSON-serializable value."),
            }),
            &["webview", "script"],
        ),
        move |args| {
            let webview = resolve(&app, args)?;
            let script = require_str(args, "script")?;
            let wrapped = format!(
                "(function() {{ try {{ var value = (function() {{ {script}\n }})(); var text = JSON.stringify(value); return text === undefined ? \"null\" : text; }} catch (e) {{ return JSON.stringify({{ evalError: String((e && e.stack) || e) }}); }} }})()"
            );
            let (sender, receiver) = mpsc::channel();
            webview
                .with_webview(move |platform| {
                    let block = RcBlock::new(move |result: *mut AnyObject, error: *mut NSError| {
                        let _ = sender.send(unsafe { string_result(result, error) });
                    });
                    // SAFETY: same contract as the snapshot tool above.
                    unsafe {
                        let wk = &*platform.inner().cast::<WKWebView>();
                        wk.evaluateJavaScript_completionHandler(
                            &NSString::from_str(&wrapped),
                            Some(&block),
                        );
                    }
                })
                .map_err(|e| e.to_string())?;
            let text = receiver
                .recv_timeout(REPLY_TIMEOUT)
                .map_err(|_| "evaluation timed out".to_string())??;
            let value = serde_json::from_str(&text).unwrap_or(Value::String(text));
            Ok(json!({"result": value}))
        },
    )
}

/// SAFETY: both pointers come straight from the completion handler; either may be null.
unsafe fn encode_png(image: *mut NSImage, error: *mut NSError) -> Result<Vec<u8>, String> {
    let Some(image) = image.as_ref() else {
        return Err(describe(error, "snapshot returned no image"));
    };
    let tiff = image.TIFFRepresentation().ok_or("no TIFF representation")?;
    let rep = NSBitmapImageRep::imageRepWithData(&tiff).ok_or("not bitmap-decodable")?;
    let png = rep
        .representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
        .ok_or("PNG encoding failed")?;
    Ok(png.to_vec())
}

/// SAFETY: both pointers come straight from the completion handler; either may be null.
unsafe fn string_result(result: *mut AnyObject, error: *mut NSError) -> Result<String, String> {
    if let Some(error) = error.as_ref() {
        return Err(error.localizedDescription().to_string());
    }
    let Some(object) = result.as_ref() else {
        return Err("evaluation returned nothing".to_string());
    };
    object
        .downcast_ref::<NSString>()
        .map(|s| s.to_string())
        .ok_or_else(|| "evaluation result was not a string".to_string())
}

/// SAFETY: `error` may be null.
unsafe fn describe(error: *mut NSError, fallback: &str) -> String {
    error
        .as_ref()
        .map_or_else(|| fallback.to_string(), |e| e.localizedDescription().to_string())
}
