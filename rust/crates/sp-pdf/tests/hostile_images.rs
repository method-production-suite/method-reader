//! A picture's declared size is a stranger's number. The decoder sizes its
//! buffers from Width and Height (and from the largest of the picture, its
//! SMask and its Mask), not from the bytes the stream carries, so a PDF of
//! a few hundred bytes could ask for gigabytes: 12,000 x 12,000 with an
//! SMask and eight bytes of data took 2.2 GB, and the old 1e9-pixel cap let
//! one ask for about 15 GB, which ends the process, past any catch_unwind.
//! An over-size picture is refused (no picture), as an undecodable one is.

use sp_pdf::{Document, PageOp, image_fact};

/// One page of text (enough lines to pass the scanned-PDF guard) painting
/// `Im1`: a 1-bit gray picture `w x h` with an 8-bit SMask `sw x sh`, each
/// stream four zero bytes.
fn pdf_with_picture(w: u32, h: u32, sw: u32, sh: u32) -> Vec<u8> {
    pdf_with_objects(vec![
        format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceGray \
             /BitsPerComponent 1 /SMask 7 0 R /Length 4 >>stream\n\0\0\0\0\nendstream"
        )
        .into_bytes(),
        format!(
            "<< /Type /XObject /Subtype /Image /Width {sw} /Height {sh} /ColorSpace /DeviceGray \
             /BitsPerComponent 8 /Length 4 >>stream\n\0\0\0\0\nendstream"
        )
        .into_bytes(),
    ])
}

/// The same page painting an 8 x 8 DCTDecode picture whose JPEG frame
/// says `w x h`.
fn pdf_with_jpeg(w: u16, h: u16) -> Vec<u8> {
    let j = jpeg(w, h);
    pdf_with_objects(vec![
        [
            format!(
                "<< /Type /XObject /Subtype /Image /Width 8 /Height 8 /ColorSpace /DeviceGray \
                 /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>stream\n",
                j.len()
            )
            .as_bytes(),
            &j,
            b"\nendstream",
        ]
        .concat(),
    ])
}

/// The page, its font, and `images` as objects 6 onward (`Im1` is 6).
fn pdf_with_objects(images: Vec<Vec<u8>>) -> Vec<u8> {
    let content =
        b"BT /F1 12 Tf 72 700 Td (INT. HOUSE - DAY) Tj ET q 400 0 0 400 100 100 cm /Im1 Do Q";
    let mut objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
          /Resources << /Font << /F1 5 0 R >> /XObject << /Im1 6 0 R >> >> >>"
            .to_vec(),
        [
            format!("<< /Length {} >>stream\n", content.len()).as_bytes(),
            content,
            b"\nendstream",
        ]
        .concat(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".to_vec(),
    ];
    objs.extend(images);
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(o);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// Whether the page's one picture decodes.
fn decodes(pdf: Vec<u8>) -> bool {
    let doc = Document::load(pdf).expect("loads");
    let mut reader = doc.reader();
    let page = reader.page(0);
    let id = page
        .ops
        .iter()
        .flatten()
        .find_map(|op| match op {
            PageOp::PaintImage { obj_id, .. } => Some(obj_id.clone()),
            _ => None,
        })
        .expect("a picture is painted");
    image_fact(&reader, &id).is_some()
}

/// A baseline gray JPEG of one 8x8 block whose frame header declares
/// `w x h`: one quantization table, one-code Huffman tables (DC category 0,
/// AC end-of-block), and the two bits of scan data that say so.
fn jpeg(w: u16, h: u16) -> Vec<u8> {
    let mut j = vec![0xff, 0xd8];
    j.extend([0xff, 0xdb, 0x00, 0x43, 0x00]);
    j.extend([1u8; 64]);
    j.extend([0xff, 0xc0, 0x00, 0x0b, 0x08]);
    j.extend(h.to_be_bytes());
    j.extend(w.to_be_bytes());
    j.extend([0x01, 0x01, 0x11, 0x00]);
    for class in [0x00, 0x10] {
        j.extend([0xff, 0xc4, 0x00, 0x14, class, 0x01]);
        j.extend([0u8; 15]);
        j.push(0x00);
    }
    j.extend([0xff, 0xda, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3f, 0x00]);
    j.extend([0x3f, 0xff, 0xd9]);
    j
}

#[test]
fn an_ordinary_jpeg_decodes() {
    assert!(decodes(pdf_with_jpeg(8, 8)));
}

#[test]
fn a_jpeg_frame_declaring_more_pixels_than_the_cap_is_refused() {
    // The picture's dictionary says 8 x 8 while the frame says
    // 30,000 x 30,000; the coefficient buffers follow the frame (2.7 GB).
    assert!(!decodes(pdf_with_jpeg(30_000, 30_000)));
}

#[test]
fn an_ordinary_picture_still_decodes() {
    assert!(decodes(pdf_with_picture(300, 200, 300, 200)));
}

#[test]
fn a_picture_declaring_more_pixels_than_the_cap_is_refused() {
    assert!(!decodes(pdf_with_picture(12_000, 12_000, 12_000, 12_000)));
}

#[test]
fn a_picture_and_its_mask_cannot_multiply_into_an_over_size_canvas() {
    // Each is small on its own; the canvas is the widest by the tallest.
    assert!(!decodes(pdf_with_picture(1, 40_000, 40_000, 1)));
}
