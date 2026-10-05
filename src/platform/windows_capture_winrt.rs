//! Keep WGC factories inside the calling apartment. The generated static
//! cache can retain a factory after the last capture worker uninitializes COM;
//! starting a second capture then dereferences the retired factory on Windows.
use windows::{core::{Interface, Result}, Graphics::{Capture::*, DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat}, SizeInt32}};

pub(super) fn supported() -> Result<bool> { unsafe {
    let factory = windows::core::factory::<GraphicsCaptureSession, IGraphicsCaptureSessionStatics>()?;
    let mut value = false;
    (factory.vtable().IsSupported)(factory.as_raw(), &mut value).map(|| value)
} }

pub(super) fn frame_pool(device: &IDirect3DDevice, format: DirectXPixelFormat, buffers: i32, size: SizeInt32) -> Result<Direct3D11CaptureFramePool> { unsafe {
    let factory = windows::core::factory::<Direct3D11CaptureFramePool, IDirect3D11CaptureFramePoolStatics2>()?;
    let mut raw = std::ptr::null_mut();
    (factory.vtable().CreateFreeThreaded)(factory.as_raw(), device.as_raw(), format, buffers, size, &mut raw).ok()?;
    if raw.is_null() { return Err(windows::Win32::Foundation::E_POINTER.into()); }
    Ok(Direct3D11CaptureFramePool::from_raw(raw))
} }
