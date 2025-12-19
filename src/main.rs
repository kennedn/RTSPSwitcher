// src/main.rs
use cairo::{Context, FontSlant, FontWeight};
use evdev::{Device, InputEventKind, Key};
use gstreamer as gst;
use gst::prelude::*;
use gst::glib::translate::ToGlibPtr;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

const DEV_PATH: &str = "/dev/input/event1";

const URL_1: &str = "rtsp://frigate.kennedn.com:8554/livingroom_sub";
const URL_2: &str = "rtsp://frigate.kennedn.com:8554/front_garden_sub";
const URL_3: &str = "rtsp://frigate.kennedn.com:8554/back_garden_sub";

fn make_rtspsrc(url: &str) -> Result<gst::Element, gst::glib::BoolError> {
    let src = gst::ElementFactory::make("rtspsrc").name("src").build()?;
    src.set_property("location", url);
    src.set_property_from_str("protocols", "tcp");
    src.set_property("latency", 0u32);
    src.set_property("drop-on-latency", true);
    Ok(src)
}

fn should_link_pad(src_pad: &gst::Pad) -> bool {
    let name = src_pad.name();
    if name.starts_with("recv_rtp_src_") {
        return true;
    }
    if name.contains("rtcp") {
        return false;
    }

    let caps = src_pad.current_caps().unwrap_or_else(|| src_pad.query_caps(None));
    if let Some(s) = caps.structure(0) {
        if s.name() != "application/x-rtp" {
            return false;
        }
        return s.get::<&str>("media").map(|m| m == "video").unwrap_or(true);
    }

    false
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    gst::init()?;

    let pipeline = gst::Pipeline::new();

    // Downstream stays constant
    let depay = gst::ElementFactory::make("rtph264depay").name("depay").build()?;
    let parse = gst::ElementFactory::make("h264parse").name("h264parse").build()?;
    let dec = gst::ElementFactory::make("avdec_h264").name("dec").build()?;
    let conv = gst::ElementFactory::make("videoconvert").name("conv").build()?;
    let overlay = gst::ElementFactory::make("cairooverlay").name("overlay").build()?;
    let sink = gst::ElementFactory::make("autovideosink").name("sink").build()?;
    sink.set_property("sync", false);

    pipeline.add_many([&depay, &parse, &dec, &conv, &overlay, &sink])?;
    gst::Element::link_many([&depay, &parse, &dec, &conv, &overlay, &sink])?;

    // We will recreate rtspsrc on switches
    let current_src: Rc<RefCell<Option<gst::Element>>> = Rc::new(RefCell::new(None));

    // Static depay sink pad for dynamic linking
    let depay_sink_pad = depay.static_pad("sink").expect("depay has no sink pad");

    // Add initial src
    {
        let src = make_rtspsrc(URL_1)?;
        pipeline.add(&src)?;
        *current_src.borrow_mut() = Some(src.clone());

        let depay_sink_pad = depay_sink_pad.clone();
        src.connect_pad_added(move |_src, src_pad| {
            if !should_link_pad(src_pad) {
                return;
            }
            if depay_sink_pad.is_linked() {
                if let Some(peer) = depay_sink_pad.peer() {
                    let _ = depay_sink_pad.unlink(&peer);
                }
            }
            let _ = src_pad.link(&depay_sink_pad);
        });
    }

    // Overlay label shared with draw callback and switching
    let label: Arc<Mutex<String>> = Arc::new(Mutex::new("Livingroom Camera".to_string()));

    // cairooverlay draw: values[1] is boxed CairoContext; boxed payload is cairo_t*
    {
        let label = Arc::clone(&label);
        overlay.connect("draw", false, move |values| {
            let gvalue_ptr =
                values[1].to_glib_none().0 as *mut gst::glib::gobject_ffi::GValue;
            if gvalue_ptr.is_null() {
                return None;
            }

            let cairo_t_ptr = unsafe {
                gst::glib::gobject_ffi::g_value_get_boxed(gvalue_ptr) as *mut cairo::ffi::cairo_t
            };
            if cairo_t_ptr.is_null() {
                return None;
            }

            let cr: Context = unsafe { Context::from_raw_none(cairo_t_ptr) };

            let (_x1, _y1, x2, y2) = cr.clip_extents().ok()?;
            let w = x2;
            let h = y2;

            // Red border
            let border_px = 10.0_f64;
            let half = border_px / 2.0;
            cr.set_source_rgb(1.0, 0.0, 0.0);
            cr.set_line_width(border_px);
            cr.rectangle(half, half, (w - border_px).max(1.0), (h - border_px).max(1.0));
            let _ = cr.stroke();

            // Text label
            let text = label.lock().unwrap().clone();
            cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Bold);
            cr.set_font_size(36.0);

            let ext = cr.text_extents(&text).ok()?;
            let pad = 12.0;
            let bx = 30.0;
            let by = 30.0;

            cr.set_source_rgba(0.0, 0.0, 0.0, 0.45);
            cr.rectangle(bx, by, ext.width() + pad * 2.0, ext.height() + pad * 2.0);
            let _ = cr.fill();

            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.move_to(bx + pad, by + pad + ext.height());
            let _ = cr.show_text(&text);

            None
        });
    }

    // Main loop / bus watch (log errors; don't quit on switching transients)
    let main_loop = gst::glib::MainLoop::new(None, false);
    let bus = pipeline.bus().expect("Pipeline without bus");
    let _bus_watch_guard = bus.add_watch(move |_bus, msg| {
        if let gst::MessageView::Error(err) = msg.view() {
            eprintln!(
                "GStreamer error from {:?}: {} ({:?})",
                err.src().map(|s| s.path_string()),
                err.error(),
                err.debug()
            );
        }
        gst::glib::ControlFlow::Continue
    })?;

    // Switching control: evdev thread -> mpsc -> main thread poll
    let (tx, rx) = mpsc::channel::<(&'static str, &'static str)>();
    {
        let pipeline = pipeline.clone();
        let current_src = Rc::clone(&current_src);
        let depay_sink_pad = depay_sink_pad.clone();
        let label = Arc::clone(&label);

        gst::glib::timeout_add_local(Duration::from_millis(20), move || {
            while let Ok((url, new_label)) = rx.try_recv() {
                *label.lock().unwrap() = new_label.to_string();

                let _ = pipeline.set_state(gst::State::Ready);

                if let Some(peer) = depay_sink_pad.peer() {
                    let _ = depay_sink_pad.unlink(&peer);
                }

                if let Some(old) = current_src.borrow_mut().take() {
                    let _ = old.set_state(gst::State::Null);
                    let _ = pipeline.remove(&old);
                }

                let new_src = match make_rtspsrc(url) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Failed to create rtspsrc for {url}: {e}");
                        let _ = pipeline.set_state(gst::State::Playing);
                        continue;
                    }
                };

                if let Err(e) = pipeline.add(&new_src) {
                    eprintln!("Failed to add new rtspsrc: {e}");
                    let _ = pipeline.set_state(gst::State::Playing);
                    continue;
                }

                let depay_sink_pad2 = depay_sink_pad.clone();
                new_src.connect_pad_added(move |_src, src_pad| {
                    if !should_link_pad(src_pad) {
                        return;
                    }
                    if depay_sink_pad2.is_linked() {
                        if let Some(peer) = depay_sink_pad2.peer() {
                            let _ = depay_sink_pad2.unlink(&peer);
                        }
                    }
                    let _ = src_pad.link(&depay_sink_pad2);
                });

                *current_src.borrow_mut() = Some(new_src);

                let _ = pipeline.set_state(gst::State::Playing);
            }
            gst::glib::ControlFlow::Continue
        });
    }

    // evdev input thread
    thread::spawn(move || {
        let mut dev = loop {
            match Device::open(DEV_PATH) {
                Ok(d) => break d,
                Err(_) => thread::sleep(Duration::from_millis(200)),
            }
        };
        let _ = dev.grab();

        loop {
            match dev.fetch_events() {
                Ok(events) => {
                    for ev in events {
                        if ev.value() != 1 {
                            continue;
                        }
                        if let InputEventKind::Key(key) = ev.kind() {
                            match key {
                                Key::KEY_NUMERIC_1 => {
                                    let _ = tx.send((URL_1, "Livingroom Camera"));
                                }
                                Key::KEY_NUMERIC_2 => {
                                    let _ = tx.send((URL_2, "Front Garden Camera"));
                                }
                                Key::KEY_NUMERIC_3 => {
                                    let _ = tx.send((URL_3, "Back Garden Camera"));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Err(_) => thread::sleep(Duration::from_millis(10)),
            }
        }
    });

    pipeline.set_state(gst::State::Playing)?;
    main_loop.run();
    let _ = pipeline.set_state(gst::State::Null);
    Ok(())
}

