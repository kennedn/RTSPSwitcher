// src/main.rs
//
// Standalone version (no examples-common).
// Features:
//  - Decode N RTSP streams (kept hot concurrently)
//  - Switch active stream using /dev/input/event2 and KEY_1..KEY_9
//  - Cairo overlay drawing via cairooverlay + pangocairo
//  - No scaling/capsfilter
//
// Run:
//   cargo run
//
// Permissions:
//   You will typically need permission to read /dev/input/event2.
//   Consider a udev rule or running with appropriate privileges.

use std::{
    ops,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};

use anyhow::{anyhow, Result};
use evdev::{Device, InputEventKind, Key};
use gstreamer as gst;
use gstreamer_video as gst_video;
use gst::prelude::*;
use pango::prelude::*;
use thiserror::Error;

const INPUT_DEVICE: &str = "/dev/input/event2";

const RTSP_URLS: &[&str] = &[
    "rtsp://frigate.kennedn.com:8554/front_garden_sub",
    "rtsp://frigate.kennedn.com:8554/back_garden_sub",
    "rtsp://frigate.kennedn.com:8554/livingroom_sub",
];

#[derive(Debug, Error)]
#[error("Received error from {src}: {error} (debug: {debug:?})")]
struct ErrorMessage {
    src: glib::GString,
    error: glib::Error,
    debug: Option<glib::GString>,
}

struct DrawingContext {
    layout: LayoutWrapper,
    info: Option<gst_video::VideoInfo>,
}

#[derive(Debug)]
struct LayoutWrapper(pango::Layout);

impl ops::Deref for LayoutWrapper {
    type Target = pango::Layout;

    fn deref(&self) -> &pango::Layout {
        assert_eq!(self.0.ref_count(), 1);
        &self.0
    }
}

// SAFETY: We ensure that there are never multiple references to the layout.
unsafe impl Send for LayoutWrapper {}

struct PipelineWithSwitch {
    pipeline: gst::Pipeline,
    selector: gst::Element,
    pads: Vec<gst::Pad>, // selector sink pads; choose via active-pad
}

fn key_to_index(key: Key) -> Option<usize> {
    match key {
        Key::KEY_1 => Some(0),
        Key::KEY_2 => Some(1),
        Key::KEY_3 => Some(2),
        Key::KEY_4 => Some(3),
        Key::KEY_5 => Some(4),
        Key::KEY_6 => Some(5),
        Key::KEY_7 => Some(6),
        Key::KEY_8 => Some(7),
        Key::KEY_9 => Some(8),
        _ => None,
    }
}

fn start_key_switch_listener(device_path: &str) -> Result<mpsc::Receiver<usize>> {
    let mut dev = Device::open(device_path)?;
    let (tx, rx) = mpsc::channel::<usize>();

    thread::spawn(move || {
        loop {
            let events = match dev.fetch_events() {
                Ok(ev) => ev,
                Err(_) => break,
            };

            for ev in events {
                // value == 1 => key press
                if ev.value() != 1 {
                    continue;
                }

                if let InputEventKind::Key(k) = ev.kind() {
                    if let Some(idx) = key_to_index(k) {
                        if tx.send(idx).is_err() {
                            return;
                        }
                    }
                }
            }
        }
    });

    Ok(rx)
}

fn create_pipeline() -> Result<PipelineWithSwitch> {
    gst::init()?;

    if RTSP_URLS.is_empty() {
        return Err(anyhow!("RTSP_URLS is empty"));
    }

    let pipeline = gst::Pipeline::default();

    // Fan-in selector
    let selector = gst::ElementFactory::make("input-selector").build()?;

    // Overlay + sink chain
    let overlay = gst::ElementFactory::make("cairooverlay").build()?;
    let post_convert = gst::ElementFactory::make("videoconvert").build()?;
    let sink = gst::ElementFactory::make("autovideosink").build()?;

    pipeline.add_many([&selector, &overlay, &post_convert, &sink])?;
    gst::Element::link_many([&selector, &overlay, &post_convert, &sink])?;

    // Build one RTSP decode branch per URL and link into selector
    let mut selector_pads: Vec<gst::Pad> = Vec::with_capacity(RTSP_URLS.len());

    for (idx, uri) in RTSP_URLS.iter().copied().enumerate() {
        let src = gst::ElementFactory::make("uridecodebin")
            .property("uri", uri)
            .build()?;

        let q = gst::ElementFactory::make("queue").build()?;
        let c = gst::ElementFactory::make("videoconvert").build()?;

        pipeline.add_many([&src, &q, &c])?;
        gst::Element::link_many([&q, &c])?;

        // Request one selector sink pad per branch
        let sel_sink_pad = selector
            .request_pad_simple("sink_%u")
            .unwrap_or_else(|| panic!("Failed to request sink pad for selector (stream {idx})"));

        // Link branch converter to selector sink pad
        c.static_pad("src")
            .unwrap_or_else(|| panic!("videoconvert missing src pad (stream {idx})"))
            .link(&sel_sink_pad)?;

        selector_pads.push(sel_sink_pad);

        // Dynamic pad linking from uridecodebin -> queue (video only)
        {
            let q_sink = q
                .static_pad("sink")
                .unwrap_or_else(|| panic!("queue missing sink pad (stream {idx})"));

            src.connect_pad_added(move |_src, src_pad| {
                let caps = src_pad.current_caps().unwrap_or_else(|| src_pad.query_caps(None));
                let is_video = caps
                    .structure(0)
                    .map(|s| s.name().starts_with("video/"))
                    .unwrap_or(false);

                if !is_video || q_sink.is_linked() {
                    return;
                }

                let _ = src_pad.link(&q_sink);
            });
        }
    }

    // Start on stream 0 explicitly
    selector.set_property("active-pad", &selector_pads[0]);

    // --- Cairo overlay setup ---
    let fontmap = pangocairo::FontMap::new();
    let context = fontmap.create_context();
    let layout = LayoutWrapper(pango::Layout::new(&context));

    let font_desc = pango::FontDescription::from_string("Sans Bold 26");
    layout.set_font_description(Some(&font_desc));
    layout.set_text("GStreamer");

    let drawer = Arc::new(Mutex::new(DrawingContext { layout, info: None }));

    let drawer_clone = drawer.clone();
    // overlay.connect("draw", false, move |args| {
    //     use std::f64::consts::PI;

    //     let drawer = drawer_clone.lock().unwrap();

    //     let cr = args[1].get::<cairo::Context>().unwrap();
    //     let timestamp = args[2].get::<gst::ClockTime>().unwrap();

    //     let info = drawer.info.as_ref().unwrap();
    //     let layout = &drawer.layout;

    //     let angle = 2.0 * PI * (timestamp % (10 * gst::ClockTime::SECOND)).nseconds() as f64
    //         / (10.0 * gst::ClockTime::SECOND.nseconds() as f64);

    //     cr.translate(
    //         f64::from(info.width()) / 2.0,
    //         f64::from(info.height()) / 2.0,
    //     );
    //     cr.rotate(angle);

    //     for i in 0..10 {
    //         cr.save().expect("Failed to save state");

    //         let angle = (360. * f64::from(i)) / 10.0;
    //         let red = (1.0 + f64::cos((angle - 60.0) * PI / 180.0)) / 2.0;
    //         cr.set_source_rgb(red, 0.0, 1.0 - red);
    //         cr.rotate(angle * PI / 180.0);

    //         pangocairo::functions::update_layout(&cr, layout);
    //         let (width, _height) = layout.size();
    //         cr.move_to(
    //             -(f64::from(width) / f64::from(pango::SCALE)) / 2.0,
    //             -(f64::from(info.height())) / 2.0,
    //         );
    //         pangocairo::functions::show_layout(&cr, layout);

    //         cr.restore().expect("Failed to restore state");
    //     }

    //     None
    // });

    overlay.connect("draw", false, move |args| {
        let drawer = drawer_clone.lock().unwrap();

        let cr = args[1].get::<cairo::Context>().unwrap();

        let info = drawer.info.as_ref().unwrap();
        let layout = &drawer.layout;

        let w = f64::from(info.width());
        let h = f64::from(info.height());

        // --- Red border ---
        cr.set_source_rgb(1.0, 0.0, 0.0);
        cr.set_line_width(6.0);

        // Inset so the full stroke stays visible
        let inset = 3.0;
        cr.rectangle(inset, inset, w - inset * 2.0, h - inset * 2.0);
        cr.stroke().expect("Failed to stroke border");

        // --- Text (top-left) ---
        cr.move_to(12.0, 12.0);
        pangocairo::functions::update_layout(&cr, layout);
        pangocairo::functions::show_layout(&cr, layout);

        None
    });


    overlay.connect("caps-changed", false, move |args| {
        let caps = args[1].get::<gst::Caps>().unwrap();

        let mut drawer = drawer.lock().unwrap();
        drawer.info = Some(gst_video::VideoInfo::from_caps(&caps).unwrap());

        None
    });

    Ok(PipelineWithSwitch {
        pipeline,
        selector,
        pads: selector_pads,
    })
}

fn main_loop(p: PipelineWithSwitch) -> Result<()> {
    let PipelineWithSwitch {
        pipeline,
        selector,
        pads,
    } = p;

    let key_rx = start_key_switch_listener(INPUT_DEVICE)?;

    pipeline.set_state(gst::State::Playing)?;

    let bus = pipeline
        .bus()
        .expect("Pipeline without bus. Shouldn't happen!");

    loop {
        // Apply any pending key presses (drain quickly)
        while let Ok(idx) = key_rx.try_recv() {
            if idx < pads.len() {
                selector.set_property("active-pad", &pads[idx]);
            }
        }

        // Keep reacting to bus while remaining responsive to key presses
        match bus.timed_pop(gst::ClockTime::from_mseconds(200)) {
            None => {
                thread::sleep(Duration::from_millis(5));
                continue;
            }
            Some(msg) => {
                use gst::MessageView;

                match msg.view() {
                    gst::MessageView::Eos(..) => break,
                    gst::MessageView::Error(err) => {
                        pipeline.set_state(gst::State::Null)?;
                        return Err(ErrorMessage {
                            src: msg
                                .src()
                                .map(|s| s.path_string())
                                .unwrap_or_else(|| glib::GString::from("UNKNOWN")),
                            error: err.error(),
                            debug: err.debug(),
                        }
                        .into());
                    }
                    _ => (),
                }
            }
        }
    }

    pipeline.set_state(gst::State::Null)?;
    Ok(())
}

fn main() {
    if let Err(e) = (|| -> Result<()> {
        let p = create_pipeline()?;
        main_loop(p)?;
        Ok(())
    })() {
        eprintln!("Error! {e}");
        std::process::exit(1);
    }
}
