//! A render bundle on a lost device is `None`, never wgpu's panic
//! (Yevkassim's crash report 3042, 2026-11-25: "Error in
//! RenderBundleEncoder::finish ... Parent device is lost", a Radeon's driver
//! reset). wgpu raises that one as a panic of its own, not through the
//! device's error handler, so `finish_bundle` must not call it once the
//! device is taken as lost. A destroyed device refuses as a lost one does.
//! The control: wgpu's own `finish` on it panics, and the guard on another
//! device (a recovery's) finishes. Needs a GPU adapter (any backend);
//! without one it says so and passes.

/// Two devices of one instance, as a recovery makes its new device on the
/// instance the lost one was made on (`viewer`'s `remake`).
fn gpus() -> Option<(wgpu::Device, wgpu::Device)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return None;
    };
    let one = pollster::block_on(adapter.request_device(&Default::default())).unwrap().0;
    let two = pollster::block_on(adapter.request_device(&Default::default())).unwrap().0;
    Some((one, two))
}

fn encoder(device: &wgpu::Device) -> wgpu::RenderBundleEncoder<'_> {
    device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
        label: Some("test"),
        color_formats: &[Some(wgpu::TextureFormat::Rgba8Unorm)],
        depth_stencil: None,
        sample_count: 1,
        multiview: None,
    })
}

#[test]
fn a_lost_devices_bundle_is_none_not_a_panic() {
    let Some((lost, fresh)) = gpus() else { return };
    assert_ne!(render::device_key(&lost), render::device_key(&fresh));
    assert!(render::finish_bundle(&lost, encoder(&lost), "test").is_some(), "the control: a live device's bundle is made");
    lost.destroy();
    // the control: wgpu's own finish panics on it (the crash)
    let raw = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        encoder(&lost).finish(&wgpu::RenderBundleDescriptor { label: None });
    }));
    let text = raw.expect_err("wgpu's finish on a lost device panics");
    let text = text.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(text.contains("Parent device is lost"), "the crash's words: {text}");
    render::set_lost_device(render::device_key(&lost));
    assert!(render::device_lost(&lost));
    assert!(render::finish_bundle(&lost, encoder(&lost), "test").is_none(), "skipped, no panic");
    // a recovery's device is not the lost one
    assert!(!render::device_lost(&fresh));
    assert!(render::finish_bundle(&fresh, encoder(&fresh), "test").is_some());
    render::set_lost_device(0);
    assert!(!render::device_lost(&lost));
}
