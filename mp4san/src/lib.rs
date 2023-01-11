use std::io::{self, BufRead, BufReader, Read, Seek};

use mp4::{skip_box, BoxHeader, BoxType, FourCC, FtypBox, MoovBox, ReadBox, WriteBox, HEADER_SIZE};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid box layout: {0}")]
    InvalidBoxLayout(&'static str),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Parse error: {0}")]
    Parse(#[from] mp4::Error),
    #[error("Unsupported box: {0}")]
    UnsupportedBox(BoxType),
    #[error("Unsupported box layout: {0}")]
    UnsupportedBoxLayout(&'static str),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(FourCC),
}

pub struct SanitizedMetadata {
    pub metadata: Box<[u8]>,
    pub data: InputSpan,
}

pub struct InputSpan {
    pub offset: u64,
    pub len: u64,
}

pub fn sanitize<R: Read + Seek>(input: R) -> Result<SanitizedMetadata, Error> {
    let mut reader = BufReader::new(input);

    let mut ftyp = None;
    let mut moov = None;
    let mut data: Option<InputSpan> = None;

    while !reader.fill_buf()?.is_empty() {
        let start_pos = reader.stream_position()?;

        // NB: Only pass `size` to other `mp4` functions and don't rely on it to be meaningful; BoxHeader actually
        // subtracts HEADER_SIZE from size in the 64-bit box size case as a hack.
        let BoxHeader { name, size: mut box_size } = BoxHeader::read(&mut reader)?;
        if box_size == 0 {
            let input_pos = reader.get_mut().stream_position()?;

            // This is the unstable Seek::stream_len()
            let input_len = reader.get_mut().seek(io::SeekFrom::End(0))?;
            if input_pos != input_len {
                reader.get_mut().seek(io::SeekFrom::Start(input_pos))?;
            }

            box_size = input_len - start_pos;
            log::info!("last box size: {box_size}");
        }

        match name {
            BoxType::FreeBox => {
                skip_box(&mut reader, box_size)?;
                log::info!("free: {box_size} bytes");

                // Try to extend any already accumulated data in case there's more mdat boxes to come.
                if let Some(data) = &mut data {
                    if data.offset + data.len == start_pos {
                        data.len += reader.stream_position()? - start_pos;
                    }
                }
            }

            BoxType::FtypBox if ftyp.is_some() => return Err(Error::InvalidBoxLayout("multiple ftyp boxes")),
            BoxType::FtypBox => {
                let read_ftyp = FtypBox::read_box(&mut reader, box_size)?;
                log::info!("ftyp: {read_ftyp:#?}");
                ftyp = Some(read_ftyp);
            }
            _ if ftyp.is_none() => return Err(Error::InvalidBoxLayout("ftyp is not the first significant box")),

            BoxType::MdatBox => {
                skip_box(&mut reader, box_size)?;
                log::info!("mdat: {box_size} bytes");

                if let Some(data) = &mut data {
                    // Try to extend already accumulated data.
                    if data.offset + data.len != start_pos {
                        return Err(Error::UnsupportedBoxLayout("discontiguous mdat boxes"));
                    }
                    data.len += reader.stream_position()? - start_pos;
                } else {
                    data = Some(InputSpan { offset: start_pos, len: reader.stream_position()? - start_pos });
                }
            }
            BoxType::MoovBox => {
                let read_moov = MoovBox::read_box(&mut reader, box_size)?;
                log::info!("moov: {read_moov:#?}");

                moov = Some(read_moov);
            }
            _ => {
                log::info!("{name}: {box_size} bytes");
                return Err(Error::UnsupportedBox(name));
            }
        }
    }

    let Some(ftyp) = ftyp else {
        return Err(Error::Parse(mp4::Error::BoxNotFound(BoxType::FtypBox)));
    };
    let FtypBox { major_brand: FourCC { value: [b'm', b'p', b'4', b'1' | b'2'] }, .. } = ftyp else {
        return Err(Error::UnsupportedFormat(ftyp.major_brand));
    };
    let Some(mut moov) = moov else {
        return Err(Error::Parse(mp4::Error::BoxNotFound(BoxType::MoovBox)));
    };
    let Some(data) = data else {
        return Err(Error::Parse(mp4::Error::BoxNotFound(BoxType::MdatBox)));
    };

    // Add a free box to pad, if one will fit, if the mdat box would move backward. If one won't fit, or if the mdat box
    // would move forward, adjust mdat offsets in stco/co64 the amount it was displaced.
    let metadata_len = ftyp.get_size() + moov.get_size();
    let mut pad_size = 0;
    match data.offset.checked_sub(metadata_len) {
        Some(0) => (),
        Some(size @ HEADER_SIZE..=u64::MAX) => pad_size = size,
        mdat_backward_displacement => {
            let mdat_forward_displacement = metadata_len.checked_sub(data.offset);
            for trak in &mut moov.traks {
                if let Some(stco) = &mut trak.mdia.minf.stbl.stco {
                    for entry in &mut stco.entries {
                        if let Some(mdat_backward_displacement) = mdat_backward_displacement {
                            *entry -= mdat_backward_displacement as u32;
                        } else if let Some(mdat_forward_displacement) = mdat_forward_displacement {
                            *entry += mdat_forward_displacement as u32;
                        }
                    }
                } else if let Some(co64) = &mut trak.mdia.minf.stbl.co64 {
                    for entry in &mut co64.entries {
                        if let Some(mdat_backward_displacement) = mdat_backward_displacement {
                            *entry -= mdat_backward_displacement;
                        } else if let Some(mdat_forward_displacement) = mdat_forward_displacement {
                            *entry += mdat_forward_displacement;
                        }
                    }
                }
            }
        }
    }

    let metadata = {
        let mut metadata = Vec::with_capacity((metadata_len + pad_size) as usize);
        ftyp.write_box(&mut metadata)?;
        moov.write_box(&mut metadata)?;
        if pad_size != 0 {
            BoxHeader { name: BoxType::FreeBox, size: pad_size }.write(&mut metadata)?;
            metadata.resize((metadata_len + pad_size) as usize, 0);
        }
        metadata.into_boxed_slice()
    };

    Ok(SanitizedMetadata { metadata, data })
}

#[cfg(test)]
mod test {
    use mp4::WriteBox;

    use super::*;

    #[test]
    fn zero_size_box() {
        let mut data = vec![];

        let ftyp = FtypBox { major_brand: (*b"mp42").into(), ..Default::default() };
        ftyp.write_box(&mut data).unwrap();

        BoxHeader { name: BoxType::MdatBox, size: 9 }.write(&mut data).unwrap();
        data.push(b'A');

        let moov_pos = data.len();
        MoovBox::default().write_box(&mut data).unwrap();
        let mut header = BoxHeader::read(&mut &data[moov_pos..]).unwrap();
        header.size = 0;
        header.write(&mut &mut data[moov_pos..]).unwrap();

        sanitize(io::Cursor::new(data)).unwrap();
    }
}
