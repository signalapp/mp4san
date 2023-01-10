use std::io::{self, BufRead, BufReader, Read, Seek};

use mp4::{skip_box, BoxHeader, BoxType, FtypBox, MoovBox, ReadBox};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Parse error: {0}")]
    Parse(#[from] mp4::Error),
}

pub fn sanitize<R: Read + Seek>(input: R) -> Result<(), Error> {
    let mut reader = BufReader::new(input);

    let mut ftyp = None;
    let mut moov = None;

    while !reader.fill_buf()?.is_empty() {
        let start_pos = reader.stream_position()?;

        let BoxHeader { name, mut size } = BoxHeader::read(&mut reader)?;
        if size == 0 {
            let input_pos = reader.get_mut().stream_position()?;

            // This is the unstable Seek::stream_len()
            let input_len = reader.get_mut().seek(io::SeekFrom::End(0))?;
            if input_pos != input_len {
                reader.get_mut().seek(io::SeekFrom::Start(input_pos))?;
            }

            size = input_len - start_pos;
            log::info!("last box size: {size}");
        }

        match name {
            BoxType::FtypBox => {
                let read_ftyp = FtypBox::read_box(&mut reader, size)?;
                log::info!("ftyp: {read_ftyp:#?}");
                ftyp = Some(read_ftyp);
            }
            BoxType::FreeBox => {
                skip_box(&mut reader, size)?;
                log::info!("free: {size} bytes");
            }
            BoxType::MdatBox => {
                skip_box(&mut reader, size)?;
                log::info!("mdat: {size} bytes");
            }
            BoxType::MoovBox => {
                let read_moov = MoovBox::read_box(&mut reader, size)?;
                log::info!("moov: {read_moov:#?}");
                moov = Some(read_moov);
            }
            _ => {
                skip_box(&mut reader, size)?;
                log::info!("{name}: {size} bytes");
            }
        }
    }

    let Some(_ftyp) = ftyp else {
        return Err(Error::Parse(mp4::Error::BoxNotFound(BoxType::FtypBox)));
    };
    let Some(_moov) = moov else {
        return Err(Error::Parse(mp4::Error::BoxNotFound(BoxType::MoovBox)));
    };

    Ok(())
}

#[cfg(test)]
mod test {
    use mp4::WriteBox;

    use super::*;

    #[test]
    fn zero_size_box() {
        let mut data = vec![];
        FtypBox::default().write_box(&mut data).unwrap();
        let moov_pos = data.len();
        MoovBox::default().write_box(&mut data).unwrap();
        let mut header = BoxHeader::read(&mut &data[moov_pos..]).unwrap();
        header.size = 0;
        header.write(&mut &mut data[moov_pos..]).unwrap();
        sanitize(io::Cursor::new(data)).unwrap();
    }
}
