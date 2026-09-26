//! Opt-in native integration smoke: actual Tauri IPC after NSPanel hosting.
//! No microphone, inference, permission prompts, or external text insertion.
#[cfg(target_os = "macos")]
static SUCCESS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(target_os = "macos")]
#[tauri::command]
fn native_probe(round: u32, app: tauri::AppHandle) -> &'static str {
    app.run_on_main_thread(move || {
        vesper_desktop_platform::hide();
        vesper_desktop_platform::show();
    })
    .expect("native presentation dispatch");
    if round == 3 {
        SUCCESS.store(true, std::sync::atomic::Ordering::Relaxed);
        println!(
            "PASS: Tauri IPC remains functional after native panel hosting and repeated show/hide."
        );
        app.exit(0);
    }
    "ok"
}

#[cfg(target_os = "macos")]
fn main() {
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "com.vesper.dictation-smoke".into();
    context.config_mut().app.windows.clear();
    context.config_mut().app.security.csp = None;
    context.config_mut().app.security.dev_csp = None;
    tauri::Builder::default()
        .register_uri_scheme_protocol("dictation-test", |_context, request| {
            eprintln!("Probe page request: {}", request.uri());
            tauri::http::Response::builder().header("Content-Type", "text/html").body(
                b"<body style='background:#171d18;color:#ecede7;font:14px system-ui'>Floating recorder validation<script>window.addEventListener('DOMContentLoaded',async()=>{try{for(let round=1;round<=3;round++){const r=await window.__TAURI_INTERNALS__.invoke('native_probe',{round});if(r!=='ok')throw Error(r);await new Promise(r=>setTimeout(r,150));}}catch(e){fetch('/error?message='+encodeURIComponent(String(e)));}});</script></body>".to_vec()
            ).unwrap()
        })
        .invoke_handler(tauri::generate_handler![native_probe])
        .setup(|app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let window = tauri::WebviewWindowBuilder::new(app, "quick-capture", tauri::WebviewUrl::CustomProtocol("dictation-test://localhost".parse()?))
                .visible(false).inner_size(440.0, 96.0).decorations(false).transparent(true).accept_first_mouse(true).build()?;
            vesper_desktop_platform::initialize(&window).map_err(std::io::Error::other)?;
            vesper_desktop_platform::show();
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(10));
                eprintln!("FAIL: native WebView IPC did not complete within ten seconds.");
                handle.exit(1);
            });
            Ok(())
        })
        .build(context).expect("build native overlay smoke")
        .run(|_, event| {
            if matches!(event, tauri::RunEvent::Exit) { vesper_desktop_platform::shutdown(); }
        });
    if !SUCCESS.load(std::sync::atomic::Ordering::Relaxed) {
        std::process::exit(1);
    }
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Native dictation overlay smoke requires macOS.");
}
