use std::{error::Error, io::Cursor, path::PathBuf};
use bti_lib::BTI;
use image::*;

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    for arg in args {
        if std::fs::exists(&arg)? {
            let pathinfo = PathBuf::from(&arg);
            let data = std::fs::read(arg)?;
            if let Ok(fmt) = image::guess_format(&data) {
                if let Ok(image) = image::load(Cursor::new(data), fmt) {
                    let image = image.into_rgba8();
                    let bti = BTI::from_image(image);
                    let bytes = bti.into_bytes(binrw::Endian::Big)?;
                    std::fs::write(pathinfo.with_extension("bti"), bytes)?;
                }
            } else {
                let bti = BTI::from_bytes(data, binrw::Endian::Big)?;
                if let Some(img) = bti.into_image() {
                    img.save_with_format(pathinfo.with_extension("png"), ImageFormat::Png)?;
                }
            }
        }
    }
    Ok(())
}
