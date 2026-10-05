//! Capture one HWND, never a rectangle of the user's other applications.
use super::{SysValue, check_active};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{io::Write, time::{Duration, Instant}};
use windows::{core::Interface, Graphics::{Capture::*, DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat}}, Win32::{Foundation::*, Graphics::{Direct3D::*, Direct3D11::*, Dxgi::{IDXGIDevice, Common::*}}, System::WinRT::{Direct3D11::*, Graphics::Capture::IGraphicsCaptureItemInterop}}};

struct Session { pool: Direct3D11CaptureFramePool, session: GraphicsCaptureSession }
impl Drop for Session { fn drop(&mut self) { let _ = self.session.Close(); let _ = self.pool.Close(); } }
struct Frame(Direct3D11CaptureFrame);
impl Drop for Frame { fn drop(&mut self) { let _ = self.0.Close(); } }
struct Mapped<'a>(&'a ID3D11DeviceContext, &'a ID3D11Texture2D);
impl Drop for Mapped<'_> { fn drop(&mut self) { unsafe { self.0.Unmap(self.1, 0); } } }
struct BoundedPng(Vec<u8>);
impl Write for BoundedPng {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > 5 * 1024 * 1024 {
            return Err(std::io::Error::other("window image exceeds the agent's transport limit"));
        }
        self.0.extend_from_slice(bytes); Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}
pub(super) fn window(hwnd: HWND, bounds: RECT) -> Result<SysValue, String> {
    let _apartment = crate::platform::windows_system::Apartment::new()?;
    capture(hwnd, bounds).map_err(|e| e.to_string())
}
fn capture(hwnd: HWND, bounds: RECT) -> windows::core::Result<SysValue> { unsafe {
    let failure = |message: &str| windows::core::Error::new(E_FAIL, message);
    if !crate::platform::windows_capture_winrt::supported()? { return Err(failure("Windows Graphics Capture is unavailable")); }
    let interop: IGraphicsCaptureItemInterop = windows::core::factory::<GraphicsCaptureItem, _>()?;
    let item: GraphicsCaptureItem = interop.CreateForWindow(hwnd)?;
    let size = item.Size()?;
    if size.Width < 1 || size.Height < 1 || size.Width > 8192 || size.Height > 8192 || i64::from(size.Width) * i64::from(size.Height) > 16_777_216 {
        return Err(failure("window capture dimensions exceed the 16-megapixel limit"));
    }
    // WGC's image and DWM's physical frame must agree before coordinates can
    // be used for input. Never quietly scale a point into a different target.
    if size.Width != bounds.right - bounds.left || size.Height != bounds.bottom - bounds.top {
        return Err(failure("capture and window frame coordinates differ; look again after the DPI or size change"));
    }
    let (mut device, mut context) = (None, None);
    D3D11CreateDevice(None, D3D_DRIVER_TYPE_HARDWARE, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut context))?;
    let device = device.ok_or(E_POINTER)?;
    let context = context.ok_or(E_POINTER)?;
    let _ = context.cast::<ID3D11Multithread>()?.SetMultithreadProtected(true);
    let runtime: IDirect3DDevice = CreateDirect3D11DeviceFromDXGIDevice(&device.cast::<IDXGIDevice>()?)?.cast()?;
    let pool = crate::platform::windows_capture_winrt::frame_pool(&runtime, DirectXPixelFormat::B8G8R8A8UIntNormalized, 1, size)?;
    let session = match pool.CreateCaptureSession(&item) { Ok(s) => s, Err(e) => { let _ = pool.Close(); return Err(e); } };
    let session = Session { pool, session };
    session.session.SetIsCursorCaptureEnabled(false)?;
    session.session.StartCapture()?;
    let start = Instant::now();
    let frame = loop {
        check_active().map_err(|e| failure(&e))?;
        if start.elapsed() > Duration::from_secs(3) { return Err(failure("the application did not provide a capture frame")); }
        match session.pool.TryGetNextFrame() {
            Ok(frame) => break Frame(frame),
            Err(e) if e.code() == E_POINTER || e.code() == S_OK => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => return Err(e),
        }
    };
    if frame.0.ContentSize()? != size { return Err(failure("the window resized during capture; look again")); }
    let source: ID3D11Texture2D = frame.0.Surface()?.cast::<IDirect3DDxgiInterfaceAccess>()?.GetInterface()?;
    let mut target = None;
    device.CreateTexture2D(&D3D11_TEXTURE2D_DESC { Width: size.Width as u32, Height: size.Height as u32, MipLevels: 1, ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM, SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 }, Usage: D3D11_USAGE_STAGING,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32, ..Default::default() }, None, Some(&mut target))?;
    let target = target.ok_or(E_POINTER)?;
    context.CopyResource(&target, &source);
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    loop {
        check_active().map_err(|e| failure(&e))?;
        match context.Map(&target, 0, D3D11_MAP_READ, D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32, Some(&mut mapped)) {
            Ok(()) => break,
            Err(e) if e.code() == windows::Win32::Graphics::Dxgi::DXGI_ERROR_WAS_STILL_DRAWING && start.elapsed() < Duration::from_secs(5) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => return Err(e),
        }
    }
    let mapping = Mapped(&context, &target);
    let stride = size.Width as usize * 4;
    if mapped.pData.is_null() || mapped.RowPitch < stride as u32 { return Err(failure("invalid window capture row pitch")); }
    let mut rgba = Vec::with_capacity(stride * size.Height as usize);
    for y in 0..size.Height as usize {
        let row = std::slice::from_raw_parts(mapped.pData.cast::<u8>().add(y * mapped.RowPitch as usize), stride);
        for bgra in row.chunks_exact(4) { rgba.extend_from_slice(&[bgra[2], bgra[1], bgra[0], bgra[3]]); }
    }
    drop(mapping);
    drop(frame);
    drop(session);
    check_active().map_err(|e| failure(&e))?;
    use image::ImageEncoder;
    let mut png = BoundedPng(Vec::new());
    image::codecs::png::PngEncoder::new(&mut png).write_image(&rgba, size.Width as u32, size.Height as u32, image::ExtendedColorType::Rgba8).map_err(|e| failure(&e.to_string()))?;
    drop(rgba);
    Ok(SysValue::Map(vec![
        ("width".into(), SysValue::Num(size.Width as f64)), ("height".into(), SysValue::Num(size.Height as f64)),
        ("data".into(), SysValue::Text(STANDARD.encode(png.0))),
    ]))
} }
