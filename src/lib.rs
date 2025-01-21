pub use gctex;
pub use binrw;
pub use binrw::Endian;
pub use image;
use gctex::*;
use binrw::prelude::*;
use std::{io::{Read, SeekFrom}, ops::{Deref, DerefMut}};
use image::*;

#[derive(Debug, Clone, Copy)]
pub struct Format(pub TextureFormat);

impl Default for Format {
    fn default() -> Self {
        Self(TextureFormat::I4)
    }
}

impl BinRead for Format {
    type Args<'a> = ();
    fn read_options<R: Read + std::io::Seek>(
            reader: &mut R,
            _: Endian,
            _: Self::Args<'_>,
        ) -> BinResult<Self> {
        let byte : u8 = reader.read_ne()?;
        if let Some(fmt) = TextureFormat::from_u32(byte as u32) {
            Ok(Self(fmt))
        } else {
            Err(binrw::Error::Custom { pos: 0, err: Box::new("Byte did not match any TextureFormats.") })
        }
    }
}

impl BinWrite for Format {
    type Args<'a> = ();
    fn write_options<W: std::io::Write + std::io::Seek>(
            &self,
            writer: &mut W,
            _: Endian,
            _: Self::Args<'_>,
        ) -> BinResult<()> {
        let byte = self.0 as u8;
        writer.write_ne(&byte)
    }
}

impl Deref for Format {
    type Target = TextureFormat;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Format {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Clone, Copy, Default, BinRead, BinWrite)]
#[brw(repr = u8)]
pub enum WrapModes
{
    #[default]
    ClampToEdge = 0,
    Repeat = 1,
    MirroredRepeat = 2,
}

#[derive(Debug, Clone, Copy, Default, BinRead, BinWrite)]
#[brw(repr = u8)]
pub enum PaletteFormats
{
    #[default]
    IA8 = 0x00,
    RGB565 = 0x01,
    RGB5A3 = 0x02,
}

#[derive(Debug, Clone, Copy, Default, BinRead, BinWrite)]
#[brw(repr = u8)]
pub enum FilterMode
{
    /* Valid in both Min and Mag Filter */
    #[default]
    Nearest = 0x0,                  // Point Sampling, No Mipmap
    Linear = 0x1,                   // Bilinear Filtering, No Mipmap

    /* Valid in only Min Filter */
    NearestMipmapNearest = 0x2,     // Point Sampling, Discrete Mipmap
    NearestMipmapLinear = 0x3,      // Bilinear Filtering, Discrete Mipmap
    LinearMipmapNearest = 0x4,      // Point Sampling, Linear MipMap
    LinearMipmapLinear = 0x5,       // Trilinear Filtering
}

#[derive(Debug, Default, Clone)]
pub struct Palette {
    pub data: Vec<u8>
}

impl Palette {
    pub fn load<R: Read>(&mut self, reader: &mut R, entrycount: u32) -> BinResult<()> {
        if entrycount == 0 { return Ok(()); }
        self.data.resize((entrycount * 2) as usize, 0);
        reader.read(&mut self.data)?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, BinRead, BinWrite)]
pub struct Header {
    pub format: Format,
    pub alpha: u8,
    pub width: u16,
    pub height: u16,
    pub wraps: WrapModes,
    pub wrapt: WrapModes,
    pub palettesenabled: u8, // binrw lacks bool support.
    pub paletteformat: PaletteFormats,
    pub palettecount: u16,
    pub palettedataoffset: u32,
    pub embeddedpalettedataoffset: u32,
    pub minfilter: FilterMode,
    pub magfilter: FilterMode,
    pub unk2: u16,
    pub mipmapcount: u8,
    pub unk3: u8,
    pub lodbais: u16,
    pub imagedataoffset: u32
}

impl Header {
    pub fn image_size(&self) -> u32 {
        compute_image_size(self.format.0, self.width as _, self.height as _)
    }
}

#[derive(Debug, Default, Clone)]
pub struct BTI {
    pub header: Header,
    pub palette: Palette,
    pub data: Vec<u8>
}

impl BTI {
    pub fn load<R: BinReaderExt>(&mut self, reader: &mut R, endian: Endian) -> BinResult<()> {
        self.header = reader.read_type(endian)?;
        reader.seek(SeekFrom::Start(self.header.palettedataoffset as _))?;
        self.palette.load(reader, self.header.palettecount as _)?;
        reader.seek(SeekFrom::Start(self.header.imagedataoffset as _))?;
        let size = self.header.image_size();
        let mut buffer = vec![0u8; size as _];
        reader.read(&mut buffer)?;
        self.data = decode(&buffer, self.header.width as _, self.header.height as _, 
            self.header.format.0, &self.palette.data, 
            self.header.paletteformat as _);
        Ok(())
    }
    pub fn into_image(self) -> Option<RgbaImage> {
        RgbaImage::from_raw(self.header.width as _, self.header.height as _, self.data)
    }
    
    pub fn from_image(image: RgbaImage) -> Self {
        let mut result = Self::default();
        result.header.format = Format(TextureFormat::CMPR);
        result.header.minfilter = FilterMode::Linear;
        result.header.magfilter = FilterMode::Linear;
        result.header.mipmapcount = 1;
        result.header.width = image.width() as _;
        result.header.height = image.height() as _;
        result.data = image.into_vec();
        result.detectandsetfittingformat();
        result
    }

    fn detectandsetfittingformat(&mut self) {
        let mut is_gray = true;
        let mut complex_alpha = false;
        let mut has_alpha = false;
        for i in 0..self.data.len() / 4 {
            let bytes = &self.data[i * 4..(i*4)+4];
            let r = bytes[0];
            let g = bytes[1];
            let b = bytes[2];
            let a = bytes[3];
            if is_gray && (r != g || g != b || b != r) {
                is_gray = false;
            }
            if a != 255 {
                has_alpha = true;
                if  a != 0 {
                    complex_alpha = true;
                }
            }
        }
        if is_gray {
            self.header.format = Format(TextureFormat::I8);
        } else if complex_alpha {
            self.header.format = Format(TextureFormat::RGB5A3);
        } else {
            self.header.format = Format(TextureFormat::CMPR)
        }
        if has_alpha {
            self.header.alpha = 0x1;
        }
    }

    pub fn write<W: BinWriterExt>(&mut self, writer: &mut W, endian: Endian) -> BinResult<()> {
        self.header.palettesenabled = match self.palette.data.is_empty() {
            true => 0,
            false => 1
        };
        self.header.palettecount = (self.palette.data.len() / 2) as _;
        self.header.palettedataoffset = match self.header.palettesenabled != 0 {
            true => 32,
            false => 0
        };
        self.header.imagedataoffset = 32 + self.palette.data.len() as u32;
        writer.write_type(&self.header, endian)?;
        writer.write(&self.palette.data)?;
        let encoded = encode(self.header.format.0, &self.data,
            self.header.width as _, self.header.height as _);
        writer.write(&encoded)?;
        let pos = writer.seek(SeekFrom::End(0))?;
        let aligned = (pos + (32 - 1)) & !(32 -1);
        let size = aligned - pos;
        let buf = vec![0x40u8; size as _];
        writer.write(&buf)?;
        Ok(())
    }

    pub fn into_bytes(mut self, endian: Endian) -> BinResult<Vec<u8>> {
        let mut writer = std::io::Cursor::new(Vec::new());
        self.write(&mut writer, endian)?;
        Ok(writer.into_inner())
    }

    pub fn from_bytes<A: AsRef<[u8]>>(data: A, endian: Endian) -> BinResult<Self> {
        let mut result = Self::default();
        let mut reader = std::io::Cursor::new(Vec::from(data.as_ref()));
        result.load(&mut reader, endian)?;
        Ok(result)
    }
}