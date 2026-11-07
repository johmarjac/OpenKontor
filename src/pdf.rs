//! Native PDF export for offers, invoices and credit notes.
//!
//! Writes PDF 1.4 directly, using the standard fonts Helvetica and Helvetica-Bold
//! (built into every PDF viewer, so nothing is embedded) with WinAnsi encoding,
//! which covers German umlauts, ß, € and „“.

use crate::i18n::{Lang, t_in};
use crate::model::*;
use crate::pdf_metrics::{HELVETICA, HELVETICA_BOLD};

// A4 in points (1 pt = 1/72 inch).
const PAGE_W: f32 = 595.28;
const PAGE_H: f32 = 841.89;
const MM: f32 = 72.0 / 25.4;
const LEFT: f32 = 25.0 * MM;
const RIGHT: f32 = PAGE_W - 20.0 * MM;
/// Content must stay above the footer.
const BOTTOM: f32 = 38.0 * MM;

type Rgb = (f32, f32, f32);
const TEXT: Rgb = (0.11, 0.15, 0.20);
const MUTED: Rgb = (0.40, 0.45, 0.52);
const ACCENT: Rgb = (0.12, 0.44, 0.55);
const RULE: Rgb = (0.80, 0.84, 0.88);

// ---- Low-level helpers ----------------------------------------------------------

/// Maps a character to its WinAnsi code; unknown characters become "?".
fn win_ansi(c: char) -> u8 {
    match c {
        ' '..='~' => c as u8,
        '\u{a0}'..='\u{ff}' => c as u32 as u8,
        '€' => 0x80,
        '‚' => 0x82,
        '„' => 0x84,
        '…' => 0x85,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '™' => 0x99,
        '\t' => b' ',
        _ => b'?',
    }
}

fn char_width(code: u8, bold: bool) -> f32 {
    let table = if bold { &HELVETICA_BOLD } else { &HELVETICA };
    table[(code.max(32) - 32) as usize] as f32 / 1000.0
}

/// Width of a text in points.
pub fn text_width(s: &str, size: f32, bold: bool) -> f32 {
    s.chars().map(|c| char_width(win_ansi(c), bold)).sum::<f32>() * size
}

/// The text as a PDF string literal in WinAnsi encoding.
fn pdf_string(s: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for c in s.chars() {
        match win_ansi(c) {
            b'(' => out.extend_from_slice(b"\\("),
            b')' => out.extend_from_slice(b"\\)"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            code => out.push(code),
        }
    }
    out.push(b')');
    out
}

/// Breaks text into lines no wider than `width`; keeps the author's line breaks.
fn wrap(s: &str, size: f32, bold: bool, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in s.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if text_width(&candidate, size, bold) <= width || line.is_empty() {
                line = candidate;
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            }
            // A single word longer than the line is cut into pieces.
            while text_width(&line, size, bold) > width && line.chars().count() > 1 {
                let mut cut = line.chars().count();
                while cut > 1 && text_width(&line.chars().take(cut).collect::<String>(), size, bold) > width {
                    cut -= 1;
                }
                lines.push(line.chars().take(cut).collect());
                line = line.chars().skip(cut).collect();
            }
        }
        lines.push(line);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Right,
}

/// One page's drawing commands.
#[derive(Default)]
struct Page {
    ops: Vec<u8>,
    /// The company logo, if this page shows it.
    logo: Option<Logo>,
}

impl Page {
    fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, color: Rgb, align: Align, s: &str) {
        if s.is_empty() {
            return;
        }
        let x = match align {
            Align::Left => x,
            Align::Right => x - text_width(s, size, bold),
        };
        let font = if bold { "F2" } else { "F1" };
        self.ops.extend_from_slice(
            format!(
                "BT {:.3} {:.3} {:.3} rg /{font} {size:.2} Tf {x:.2} {y:.2} Td ",
                color.0, color.1, color.2
            )
            .as_bytes(),
        );
        self.ops.extend_from_slice(&pdf_string(s));
        self.ops.extend_from_slice(b" Tj ET\n");
    }

    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Rgb) {
        self.ops.extend_from_slice(
            format!(
                "{:.3} {:.3} {:.3} RG {width:.2} w {x1:.2} {y1:.2} m {x2:.2} {y2:.2} l S\n",
                color.0, color.1, color.2
            )
            .as_bytes(),
        );
    }

    /// Draws the logo (image /Im1) into a box with its lower left corner at x, y.
    fn image(&mut self, logo: &Logo, x: f32, y: f32, w: f32, h: f32) {
        self.logo = Some(logo.clone());
        self.ops.extend_from_slice(format!("q {w:.2} 0 0 {h:.2} {x:.2} {y:.2} cm /Im1 Do Q\n").as_bytes());
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgb) {
        self.ops.extend_from_slice(
            format!(
                "{:.3} {:.3} {:.3} rg {x:.2} {y:.2} {w:.2} {h:.2} re f\n",
                color.0, color.1, color.2
            )
            .as_bytes(),
        );
    }
}

/// Assembles pages into a complete PDF file.
fn write_pdf(pages: &[Page], title: &str, author: &str) -> Vec<u8> {
    write_pdf_with(pages, title, author, None)
}

/// What turns a PDF into a ZUGFeRD / Factur-X invoice: the XML and its profile name.
struct Attachment<'a> {
    xml: &'a [u8],
    conformance: &'a str,
}

static FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Regular.ttf");
static FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Bold.ttf");
static SRGB_PROFILE: &[u8] = include_bytes!("../assets/icc/sRGB-v4.icc");

/// The character a WinAnsi code stands for (for looking up glyph widths in a font file).
fn win_ansi_char(code: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
        '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    match code {
        0x80..=0x9F => HIGH[(code - 0x80) as usize],
        _ => code as char,
    }
}

fn deflate(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    e.write_all(bytes).expect("in memory");
    e.finish().expect("in memory")
}

/// A text string as UTF-16 (needed when it may contain characters outside PDFDocEncoding).
fn utf16_string(s: &str) -> String {
    let mut hex = String::from("<FEFF");
    for u in s.encode_utf16() {
        hex.push_str(&format!("{u:04X}"));
    }
    hex.push('>');
    hex
}

fn xml_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Font dictionary, descriptor and file for an embedded TrueType font (PDF/A requires embedding).
fn embedded_font(font: &[u8], name: &str, bold: bool) -> (String, String) {
    let face = ttf_parser::Face::parse(font, 0).expect("bundled font");
    let scale = 1000.0 / face.units_per_em() as f32;
    let widths: Vec<String> = (32u8..=255)
        .map(|code| {
            let w = face
                .glyph_index(win_ansi_char(code))
                .and_then(|g| face.glyph_hor_advance(g))
                .or_else(|| face.glyph_hor_advance(ttf_parser::GlyphId(0)))
                .unwrap_or(0);
            ((w as f32) * scale).round().to_string()
        })
        .collect();
    let b = face.global_bounding_box();
    let s = |v: i16| ((v as f32) * scale).round() as i32;
    let font_dict = format!(
        "<< /Type /Font /Subtype /TrueType /BaseFont /{name} /FirstChar 32 /LastChar 255 /Widths [{}] /Encoding /WinAnsiEncoding /FontDescriptor {{FD}} 0 R >>",
        widths.join(" ")
    );
    let descriptor = format!(
        "<< /Type /FontDescriptor /FontName /{name} /Flags 32 /FontBBox [{} {} {} {}] /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight {} /StemV {} /FontFile2 {{FF}} 0 R >>",
        s(b.x_min),
        s(b.y_min),
        s(b.x_max),
        s(b.y_max),
        s(face.ascender()),
        s(face.descender()),
        s(face.capital_height().unwrap_or(face.ascender())),
        if bold { 120 } else { 80 }
    );
    (font_dict, descriptor)
}

fn write_pdf_with(pages: &[Page], title: &str, author: &str, attach: Option<Attachment<'_>>) -> Vec<u8> {
    let pdfa = attach.is_some();
    let mut out: Vec<u8> = if pdfa { b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec() } else { b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec() };
    let mut offsets: Vec<usize> = Vec::new();
    // Object numbers: 1 catalog, 2 pages, 3 font regular, 4 font bold, 5 info,
    // then per page: page object and content stream; for PDF/A the extra objects follow.
    let n_pages = pages.len();
    let page_obj = |i: usize| 6 + i * 2;
    let content_obj = |i: usize| 7 + i * 2;
    let extra = 6 + n_pages * 2;
    let (fd1, ff1, fd2, ff2, meta, icc, ef, fs) = (extra, extra + 1, extra + 2, extra + 3, extra + 4, extra + 5, extra + 6, extra + 7);

    let obj = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
        offsets.push(out.len());
        let n = offsets.len();
        out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    let stream = |dict: &str, data: &[u8]| -> Vec<u8> {
        let mut v = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
        v.extend_from_slice(data);
        v.extend_from_slice(b"\nendstream");
        v
    };

    let now = chrono::Local::now();
    let offset = now.format("%z").to_string();
    let pdf_date = format!("D:{}{}'{}'", now.format("%Y%m%d%H%M%S"), &offset[..3], &offset[3..]);
    let xmp_date = now.format("%Y-%m-%dT%H:%M:%S%:z").to_string();

    if let Some(a) = &attach {
        let _ = a;
        obj(
            &mut out,
            &mut offsets,
            format!(
                "<< /Type /Catalog /Pages 2 0 R /Lang (de-DE) /Metadata {meta} 0 R \
                 /OutputIntents [<< /Type /OutputIntent /S /GTS_PDFA1 /OutputConditionIdentifier (sRGB IEC61966-2.1) /Info (sRGB IEC61966-2.1) /DestOutputProfile {icc} 0 R >>] \
                 /Names << /EmbeddedFiles << /Names [(factur-x.xml) {fs} 0 R] >> >> /AF [{fs} 0 R] >>"
            )
            .as_bytes(),
        );
    } else {
        obj(&mut out, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
    }
    let kids: Vec<String> = (0..n_pages).map(|i| format!("{} 0 R", page_obj(i))).collect();
    obj(
        &mut out,
        &mut offsets,
        format!("<< /Type /Pages /Kids [{}] /Count {n_pages} >>", kids.join(" ")).as_bytes(),
    );
    let fonts = pdfa.then(|| (embedded_font(FONT_REGULAR, "LiberationSans", false), embedded_font(FONT_BOLD, "LiberationSans-Bold", true)));
    match &fonts {
        Some(((r, _), (b, _))) => {
            obj(&mut out, &mut offsets, r.replace("{FD}", &fd1.to_string()).as_bytes());
            obj(&mut out, &mut offsets, b.replace("{FD}", &fd2.to_string()).as_bytes());
        }
        None => {
            obj(&mut out, &mut offsets, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>");
            obj(&mut out, &mut offsets, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>");
        }
    }
    let info = if pdfa {
        format!(
            "<< /Title {} /Author {} /Creator (OpenKontor) /Producer (OpenKontor) /CreationDate ({pdf_date}) /ModDate ({pdf_date}) >>",
            utf16_string(title),
            utf16_string(author)
        )
        .into_bytes()
    } else {
        let mut info = b"<< /Title ".to_vec();
        info.extend_from_slice(&pdf_string(title));
        info.extend_from_slice(b" /Author ");
        info.extend_from_slice(&pdf_string(author));
        info.extend_from_slice(b" /Producer (OpenKontor) ");
        info.extend_from_slice(format!("/CreationDate (D:{}) >>", now.format("%Y%m%d%H%M%S")).as_bytes());
        info
    };
    obj(&mut out, &mut offsets, &info);
    // The logo is one image object after all others, shared by the pages.
    let logo = pages.iter().find_map(|p| p.logo.clone());
    let image_obj = if pdfa { extra + 8 } else { extra };
    for (i, page) in pages.iter().enumerate() {
        let xobject = if page.logo.is_some() { format!(" /XObject << /Im1 {image_obj} 0 R >>") } else { String::new() };
        obj(
            &mut out,
            &mut offsets,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] \
                 /Resources << /Font << /F1 3 0 R /F2 4 0 R >>{xobject} >> /Contents {} 0 R >>",
                content_obj(i)
            )
            .as_bytes(),
        );
        obj(&mut out, &mut offsets, &stream("", &page.ops));
    }

    let mut id_source = Vec::new();
    if let (Some(a), Some(((_, d1), (_, d2)))) = (&attach, &fonts) {
        for (descriptor, file, data) in [(d1, ff1, FONT_REGULAR), (d2, ff2, FONT_BOLD)] {
            obj(&mut out, &mut offsets, descriptor.replace("{FF}", &file.to_string()).as_bytes());
            obj(&mut out, &mut offsets, &stream(&format!("/Length1 {} /Filter /FlateDecode", data.len()), &deflate(data)));
        }
        let xmp = format!(
            r#"<?xpacket begin="{bom}" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/">
   <pdfaid:part>3</pdfaid:part>
   <pdfaid:conformance>B</pdfaid:conformance>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/">
   <dc:format>application/pdf</dc:format>
   <dc:title><rdf:Alt><rdf:li xml:lang="x-default">{title}</rdf:li></rdf:Alt></dc:title>
   <dc:creator><rdf:Seq><rdf:li>{author}</rdf:li></rdf:Seq></dc:creator>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:pdf="http://ns.adobe.com/pdf/1.3/">
   <pdf:Producer>OpenKontor</pdf:Producer>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/">
   <xmp:CreatorTool>OpenKontor</xmp:CreatorTool>
   <xmp:CreateDate>{date}</xmp:CreateDate>
   <xmp:ModifyDate>{date}</xmp:ModifyDate>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:fx="urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#">
   <fx:DocumentType>INVOICE</fx:DocumentType>
   <fx:DocumentFileName>factur-x.xml</fx:DocumentFileName>
   <fx:Version>1.0</fx:Version>
   <fx:ConformanceLevel>{level}</fx:ConformanceLevel>
  </rdf:Description>
  <rdf:Description rdf:about="" xmlns:pdfaExtension="http://www.aiim.org/pdfa/ns/extension/" xmlns:pdfaSchema="http://www.aiim.org/pdfa/ns/schema#" xmlns:pdfaProperty="http://www.aiim.org/pdfa/ns/property#">
   <pdfaExtension:schemas>
    <rdf:Bag>
     <rdf:li rdf:parseType="Resource">
      <pdfaSchema:schema>Factur-X PDFA Extension Schema</pdfaSchema:schema>
      <pdfaSchema:namespaceURI>urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#</pdfaSchema:namespaceURI>
      <pdfaSchema:prefix>fx</pdfaSchema:prefix>
      <pdfaSchema:property>
       <rdf:Seq>
        <rdf:li rdf:parseType="Resource"><pdfaProperty:name>DocumentFileName</pdfaProperty:name><pdfaProperty:valueType>Text</pdfaProperty:valueType><pdfaProperty:category>external</pdfaProperty:category><pdfaProperty:description>The name of the embedded XML document</pdfaProperty:description></rdf:li>
        <rdf:li rdf:parseType="Resource"><pdfaProperty:name>DocumentType</pdfaProperty:name><pdfaProperty:valueType>Text</pdfaProperty:valueType><pdfaProperty:category>external</pdfaProperty:category><pdfaProperty:description>The type of the hybrid document in capital letters, e.g. INVOICE or ORDER</pdfaProperty:description></rdf:li>
        <rdf:li rdf:parseType="Resource"><pdfaProperty:name>Version</pdfaProperty:name><pdfaProperty:valueType>Text</pdfaProperty:valueType><pdfaProperty:category>external</pdfaProperty:category><pdfaProperty:description>The actual version of the standard applying to the embedded XML document</pdfaProperty:description></rdf:li>
        <rdf:li rdf:parseType="Resource"><pdfaProperty:name>ConformanceLevel</pdfaProperty:name><pdfaProperty:valueType>Text</pdfaProperty:valueType><pdfaProperty:category>external</pdfaProperty:category><pdfaProperty:description>The conformance level of the embedded XML document</pdfaProperty:description></rdf:li>
       </rdf:Seq>
      </pdfaSchema:property>
     </rdf:li>
    </rdf:Bag>
   </pdfaExtension:schemas>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#,
            bom = '\u{feff}',
            title = xml_text(title),
            author = xml_text(author),
            date = xmp_date,
            level = a.conformance,
        );
        obj(&mut out, &mut offsets, &stream("/Type /Metadata /Subtype /XML", xmp.as_bytes()));
        obj(&mut out, &mut offsets, &stream("/N 3 /Filter /FlateDecode", &deflate(SRGB_PROFILE)));
        obj(
            &mut out,
            &mut offsets,
            &stream(
                &format!("/Type /EmbeddedFile /Subtype /text#2Fxml /Params << /Size {} /ModDate ({pdf_date}) >>", a.xml.len()),
                a.xml,
            ),
        );
        obj(
            &mut out,
            &mut offsets,
            format!(
                "<< /Type /Filespec /F (factur-x.xml) /UF (factur-x.xml) /Desc (Factur-X/ZUGFeRD Rechnungsdaten) \
                 /AFRelationship /Alternative /EF << /F {ef} 0 R /UF {ef} 0 R >> >>"
            )
            .as_bytes(),
        );
        id_source.extend_from_slice(a.xml);
    }
    if let Some(l) = &logo {
        let pixels = decode_base64(&l.rgb);
        obj(
            &mut out,
            &mut offsets,
            &stream(
                &format!(
                    "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode",
                    l.width, l.height
                ),
                &pixels,
            ),
        );
    }
    id_source.extend_from_slice(title.as_bytes());
    id_source.extend_from_slice(pdf_date.as_bytes());

    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for off in &offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    let id: String = {
        use sha2::Digest;
        sha2::Sha256::digest(&id_source).iter().take(16).map(|b| format!("{b:02X}")).collect()
    };
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R /ID [<{id}> <{id}>] >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// A ZUGFeRD / Factur-X invoice: the normal invoice PDF as PDF/A-3 with the e-invoice
/// data (EN 16931) embedded. Fails with the list of missing details.
pub fn zugferd_pdf(data: &Data, doc: &Document) -> Result<Vec<u8>, Vec<String>> {
    let xml = crate::xrechnung::build(data, doc, crate::xrechnung::Profile::En16931)?;
    let (pages, title, author) = document_pages(data, doc);
    Ok(write_pdf_with(&pages, &title, &author, Some(Attachment { xml: xml.as_bytes(), conformance: "EN 16931" })))
}

// ---- Document layout ------------------------------------------------------------

/// Column layout of the item table: (left edge, right edge).
struct Cols {
    pos: f32,
    text: (f32, f32),
    qty: f32,
    price: f32,
    vat: f32,
    total: f32,
}

fn cols(small: bool) -> Cols {
    Cols {
        pos: LEFT,
        text: (LEFT + 26.0, if small { RIGHT - 230.0 } else { RIGHT - 270.0 }),
        qty: if small { RIGHT - 150.0 } else { RIGHT - 190.0 },
        price: if small { RIGHT - 75.0 } else { RIGHT - 115.0 },
        vat: RIGHT - 75.0,
        total: RIGHT,
    }
}

fn table_header(p: &mut Page, y: f32, c: &Cols, small: bool, lang: Lang) -> f32 {
    let s = 8.5;
    let l = |de| t_in(lang, de);
    p.text(c.pos, y, s, true, TEXT, Align::Left, l("Pos."));
    p.text(c.text.0, y, s, true, TEXT, Align::Left, l("Bezeichnung"));
    p.text(c.qty, y, s, true, TEXT, Align::Right, l("Menge"));
    p.text(c.price, y, s, true, TEXT, Align::Right, l("Einzelpreis"));
    if !small {
        p.text(c.vat, y, s, true, TEXT, Align::Right, l("USt."));
    }
    p.text(c.total, y, s, true, TEXT, Align::Right, l("Gesamt"));
    p.line(LEFT, y - 5.0, RIGHT, y - 5.0, 0.8, TEXT);
    y - 18.0
}

fn footer(p: &mut Page, company: &Company, page: usize, pages: usize, lang: Lang) {
    let l = |de| t_in(lang, de);
    let y = 26.0 * MM;
    p.line(LEFT, y + 10.0, RIGHT, y + 10.0, 0.5, RULE);
    let col_w = (RIGHT - LEFT) / 4.0;
    let blocks: [Vec<String>; 4] = [
        vec![company.name.clone(), company.owner.clone(), company.street.clone(), company.zip_city.clone()],
        vec![
            if company.phone.is_empty() { String::new() } else { format!("{} {}", l("Tel."), company.phone) },
            company.email.clone(),
        ],
        vec![
            if company.tax_number.is_empty() { String::new() } else { format!("{} {}", l("Steuernr."), company.tax_number) },
            if company.vat_id.is_empty() { String::new() } else { format!("{} {}", l("USt-IdNr."), company.vat_id) },
        ],
        vec![
            company.bank.clone(),
            if company.iban.is_empty() { String::new() } else { format!("IBAN {}", company.iban) },
            if company.bic.is_empty() { String::new() } else { format!("BIC {}", company.bic) },
        ],
    ];
    for (i, block) in blocks.iter().enumerate() {
        let mut ly = y;
        for line in block.iter().filter(|l| !l.is_empty()) {
            p.text(LEFT + col_w * i as f32, ly, 7.0, false, MUTED, Align::Left, line);
            ly -= 9.0;
        }
    }
    if pages > 1 {
        p.text(RIGHT, 12.0 * MM, 7.5, false, MUTED, Align::Right, &crate::i18n::fill(l("Seite {} von {}"), &[&page, &pages]));
    }
}

/// Company name, address field (DIN 5008 window position) and the details box on the right.
/// Returns where the details box ends.
fn letterhead(p: &mut Page, c: &Company, contact: Option<&Contact>, meta: &[(String, String)]) -> f32 {
    let y = PAGE_H - 22.0 * MM;
    p.text(LEFT, y, 18.0, true, ACCENT, Align::Left, &c.name);
    p.rect(LEFT, y - 9.0, RIGHT - LEFT, 2.0, ACCENT);
    // The logo sits top right above the line, at most 18 mm high and 60 mm wide.
    if let Some(logo) = &c.logo
        && logo.width > 0
        && logo.height > 0
    {
        let ratio = logo.width as f32 / logo.height as f32;
        let mut h = 18.0 * MM;
        let mut w = h * ratio;
        if w > 60.0 * MM {
            w = 60.0 * MM;
            h = w / ratio;
        }
        p.image(logo, RIGHT - w, y - 4.0, w, h);
    }

    let mut my = PAGE_H - 50.0 * MM;
    for (label, value) in meta {
        p.text(RIGHT - 120.0, my, 9.0, false, MUTED, Align::Left, label);
        p.text(RIGHT, my, 9.0, true, TEXT, Align::Right, value);
        my -= 13.0;
    }

    let mut y = PAGE_H - 50.0 * MM;
    let sender = [c.name.as_str(), c.street.as_str(), c.zip_city.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ");
    p.text(LEFT, y, 7.0, false, MUTED, Align::Left, &sender);
    p.line(LEFT, y - 2.0, LEFT + text_width(&sender, 7.0, false), y - 2.0, 0.4, MUTED);
    y -= 16.0;
    if let Some(k) = contact {
        for line in [&k.company, &k.person, &k.street, &k.zip_city] {
            if !line.is_empty() {
                p.text(LEFT, y, 10.5, false, TEXT, Align::Left, line);
                y -= 13.5;
            }
        }
        if !k.country.is_empty() && !matches!(k.country.as_str(), "Deutschland" | "Germany") {
            p.text(LEFT, y, 10.5, false, TEXT, Align::Left, &k.country.to_uppercase());
        }
    }
    my
}

/// A payment reminder (Zahlungserinnerung or Mahnung) for an open invoice.
pub fn reminder_pdf(data: &Data, doc: &Document, r: &ReminderRecord) -> Vec<u8> {
    let c = &data.company;
    let small = c.small_business;
    let contact = data.contact(doc.contact_id);
    let lang = contact.map(|k| k.language).unwrap_or_default();
    let l = |de| t_in(lang, de);
    let money = |v: Cents| fmt_money_in(lang, v);
    let date = |d: chrono::NaiveDate| fmt_date_in(lang, d);
    let gross = doc.totals(small).gross;
    let earlier: Vec<&ReminderRecord> = doc.reminders.iter().filter(|x| x.date <= r.date && *x != r).collect();

    let mut p = Page::default();
    let mut meta = vec![(l("Datum").to_string(), date(r.date))];
    if let Some(k) = contact
        && !k.number.is_empty()
    {
        meta.push((l("Kunden-Nr.").into(), k.number.clone()));
    }
    let my = letterhead(&mut p, c, contact, &meta);
    let mut y = (PAGE_H - 98.0 * MM).min(my - 20.0);
    p.text(LEFT, y, 15.0, true, TEXT, Align::Left, l(reminder_title(r.level)));
    y -= 22.0;

    let intro = match r.level {
        0 => l("sicher ist es Ihrer Aufmerksamkeit entgangen, dass die folgende Rechnung noch offen ist. Bitte überweisen Sie den Betrag bis zum unten genannten Datum."),
        1 => l("trotz unserer Zahlungserinnerung konnten wir zu der folgenden Rechnung noch keinen Zahlungseingang feststellen. Bitte überweisen Sie den offenen Betrag bis zum unten genannten Datum."),
        2 => l("leider ist die folgende Rechnung trotz Mahnung weiterhin offen. Wir bitten Sie dringend, den offenen Betrag bis zum unten genannten Datum zu überweisen."),
        _ => l("trotz mehrfacher Erinnerung ist die folgende Rechnung noch immer nicht bezahlt. Geht der offene Betrag nicht bis zum unten genannten Datum ein, werden wir die Forderung ohne weitere Ankündigung gerichtlich geltend machen oder an ein Inkassounternehmen übergeben."),
    };
    let greeting = match (lang, contact.map(|k| k.person.trim()).unwrap_or("")) {
        (Lang::De, "") => "Sehr geehrte Damen und Herren,".to_string(),
        (Lang::De, name) => format!("Guten Tag {name},"),
        (Lang::En, "") => "Dear Sir or Madam,".to_string(),
        (Lang::En, name) => format!("Dear {name},"),
    };
    p.text(LEFT, y, 10.0, false, TEXT, Align::Left, &greeting);
    y -= 18.0;
    for line in wrap(intro, 10.0, false, RIGHT - LEFT) {
        p.text(LEFT, y, 10.0, false, TEXT, Align::Left, &line);
        y -= 13.5;
    }
    y -= 12.0;

    // The open item and the fees.
    let s = 8.5;
    p.text(LEFT, y, s, true, TEXT, Align::Left, l("Rechnung"));
    p.text(LEFT + 120.0, y, s, true, TEXT, Align::Left, l("Datum"));
    p.text(LEFT + 210.0, y, s, true, TEXT, Align::Left, l("Fällig am"));
    p.text(RIGHT, y, s, true, TEXT, Align::Right, l("Betrag"));
    p.line(LEFT, y - 5.0, RIGHT, y - 5.0, 0.8, TEXT);
    y -= 18.0;
    p.text(LEFT, y, 9.5, false, TEXT, Align::Left, &doc.number);
    p.text(LEFT + 120.0, y, 9.5, false, TEXT, Align::Left, &date(doc.date));
    p.text(LEFT + 210.0, y, 9.5, false, TEXT, Align::Left, &date(doc.due_date()));
    p.text(RIGHT, y, 9.5, false, TEXT, Align::Right, &money(gross));
    y -= 16.0;
    let mut total = gross;
    for e in &earlier {
        if e.fee > 0 {
            p.text(LEFT, y, 9.5, false, TEXT, Align::Left, &crate::i18n::fill(l("Mahngebühr {} vom {}"), &[&l(reminder_title(e.level)), &date(e.date)]));
            p.text(RIGHT, y, 9.5, false, TEXT, Align::Right, &money(e.fee));
            total += e.fee;
            y -= 16.0;
        }
    }
    if r.fee > 0 {
        p.text(LEFT, y, 9.5, false, TEXT, Align::Left, l("Mahngebühr dieser Mahnung"));
        p.text(RIGHT, y, 9.5, false, TEXT, Align::Right, &money(r.fee));
        total += r.fee;
        y -= 16.0;
    }
    p.line(RIGHT - 190.0, y + 8.0, RIGHT, y + 8.0, 1.2, TEXT);
    y -= 6.0;
    p.text(RIGHT - 190.0, y, 11.5, true, TEXT, Align::Left, l("Offener Betrag"));
    p.text(RIGHT, y, 11.5, true, TEXT, Align::Right, &money(total));
    y -= 30.0;

    let pay = crate::i18n::fill(l("Bitte überweisen Sie {} bis zum {} auf unser Konto:"), &[&money(total), &date(r.due)]);
    for line in wrap(&pay, 10.0, true, RIGHT - LEFT) {
        p.text(LEFT, y, 10.0, true, TEXT, Align::Left, &line);
        y -= 13.5;
    }
    for line in [
        c.bank.clone(),
        if c.iban.is_empty() { String::new() } else { format!("IBAN {}", c.iban) },
        if c.bic.is_empty() { String::new() } else { format!("BIC {}", c.bic) },
        crate::i18n::fill(l("Verwendungszweck: {}"), &[&doc.number]),
    ] {
        if !line.is_empty() {
            p.text(LEFT, y, 10.0, false, TEXT, Align::Left, &line);
            y -= 13.5;
        }
    }
    y -= 10.0;
    for line in wrap(l("Sollten Sie die Zahlung inzwischen veranlasst haben, betrachten Sie dieses Schreiben bitte als gegenstandslos."), 9.0, false, RIGHT - LEFT) {
        p.text(LEFT, y, 9.0, false, MUTED, Align::Left, &line);
        y -= 12.0;
    }
    footer(&mut p, c, 1, 1, lang);
    let title = format!("{} {}", l(reminder_title(r.level)), doc.number);
    write_pdf(&[p], &title, &c.name)
}

/// A file name like "1. Mahnung RE-2026-0012.pdf".
pub fn reminder_file_name(doc: &Document, r: &ReminderRecord) -> String {
    let base = file_name(doc);
    let number = base.split_once(' ').map(|(_, n)| n).unwrap_or(&base);
    format!("{} {number}", crate::i18n::t(reminder_title(r.level)))
}

/// The finished PDF for an offer, invoice or credit note.
pub fn document_pdf(data: &Data, doc: &Document) -> Vec<u8> {
    let (pages, title, author) = document_pages(data, doc);
    write_pdf(&pages, &title, &author)
}

/// The laid-out pages of a document, with title and author for the file's details.
fn document_pages(data: &Data, doc: &Document) -> (Vec<Page>, String, String) {
    let c = &data.company;
    let small = c.small_business;
    let contact = data.contact(doc.contact_id);
    let totals = doc.totals(small);
    let lang = contact.map(|k| k.language).unwrap_or_default();
    let l = |de| t_in(lang, de);
    let money = |c: Cents| fmt_money_in(lang, c);
    let date = |d: chrono::NaiveDate| fmt_date_in(lang, d);
    let kind = l(doc.kind.name());
    let number = if doc.number.is_empty() { l("ENTWURF").to_string() } else { doc.number.clone() };
    let sign = if doc.kind == DocKind::Gutschrift { -1 } else { 1 };
    let col = cols(small);

    let mut pages: Vec<Page> = Vec::new();
    let mut p = Page::default();

    // Details box on the right.
    let mut meta: Vec<(String, String)> = vec![
        (crate::i18n::fill(l("{}-Nr."), &[&kind]), number.clone()),
        (l("Datum").into(), date(doc.date)),
    ];
    if !doc.service_date.trim().is_empty() {
        meta.push((l("Leistungsdatum").into(), doc.service_date.clone()));
    }
    match doc.kind {
        DocKind::Rechnung => meta.push((l("Fällig am").into(), date(doc.due_date()))),
        DocKind::Angebot => meta.push((l("Gültig bis").into(), date(doc.due_date()))),
        _ => {}
    }
    if let Some(k) = contact
        && !k.number.is_empty()
        && doc.kind != DocKind::Bestellung
    {
        meta.push((l("Kunden-Nr.").into(), k.number.clone()));
    }
    if let Some(src) = doc.source_id.and_then(|id| data.documents.iter().find(|d| d.id == id))
        && !src.number.is_empty()
        && matches!(doc.kind, DocKind::Lieferschein | DocKind::Auftragsbestaetigung)
    {
        meta.push((crate::i18n::fill(l("{}-Nr."), &[&l(src.kind.name())]), src.number.clone()));
    }
    // A delivery note shows quantities only.
    let quantities_only = doc.kind == DocKind::Lieferschein;
    let my = letterhead(&mut p, c, contact, &meta);

    // Title and opening text.
    let mut y = (PAGE_H - 98.0 * MM).min(my - 20.0);
    let title = if doc.title.is_empty() { kind } else { &doc.title };
    p.text(LEFT, y, 15.0, true, TEXT, Align::Left, title);
    y -= 20.0;
    for line in wrap(&doc.intro, 10.0, false, RIGHT - LEFT) {
        p.text(LEFT, y, 10.0, false, TEXT, Align::Left, &line);
        y -= 13.5;
    }
    y -= 8.0;

    // Item table, continued on further pages when needed.
    let small = small || quantities_only;
    let col = if quantities_only { Cols { qty: RIGHT, ..cols(true) } } else { col };
    let header = |p: &mut Page, y: f32| {
        if quantities_only {
            let s = 8.5;
            p.text(col.pos, y, s, true, TEXT, Align::Left, l("Pos."));
            p.text(col.text.0, y, s, true, TEXT, Align::Left, l("Bezeichnung"));
            p.text(RIGHT, y, s, true, TEXT, Align::Right, l("Menge"));
            p.line(LEFT, y - 5.0, RIGHT, y - 5.0, 0.8, TEXT);
            y - 18.0
        } else {
            table_header(p, y, &col, small, lang)
        }
    };
    y = header(&mut p, y);
    for (i, l) in doc.lines.iter().enumerate() {
        let mut text_lines = wrap(&l.text, 9.5, false, col.text.1 - col.text.0);
        if text_lines.is_empty() {
            text_lines.push(String::new());
        }
        if l.discount_pct > 0.0 {
            text_lines.push(crate::i18n::fill(t_in(lang, "abzgl. {} % Rabatt"), &[&fmt_qty_in(lang, l.discount_pct)]));
        }
        let row_h = text_lines.len() as f32 * 12.0 + 7.0;
        if y - row_h < BOTTOM {
            pages.push(std::mem::take(&mut p));
            y = PAGE_H - 25.0 * MM;
            p.text(LEFT, y, 9.0, false, MUTED, Align::Left, &crate::i18n::fill(t_in(lang, "{} {} – Fortsetzung"), &[&kind, &number]));
            y -= 22.0;
            y = header(&mut p, y);
        }
        p.text(col.pos, y, 9.5, false, TEXT, Align::Left, &(i + 1).to_string());
        for (k, t) in text_lines.iter().enumerate() {
            let is_discount = l.discount_pct > 0.0 && k == text_lines.len() - 1;
            p.text(
                col.text.0,
                y - k as f32 * 12.0,
                if is_discount { 8.0 } else { 9.5 },
                false,
                if is_discount { MUTED } else { TEXT },
                Align::Left,
                t,
            );
        }
        p.text(col.qty, y, 9.5, false, TEXT, Align::Right, &format!("{} {}", fmt_qty_in(lang, l.qty), l.unit));
        if !quantities_only {
            p.text(col.price, y, 9.5, false, TEXT, Align::Right, &money(l.price_net));
            if !small {
                p.text(col.vat, y, 9.5, false, TEXT, Align::Right, &format!("{} %", l.vat));
            }
            p.text(col.total, y, 9.5, false, TEXT, Align::Right, &money(sign * l.net()));
        }
        y -= row_h;
        p.line(LEFT, y + 9.0, RIGHT, y + 9.0, 0.4, RULE);
    }

    if quantities_only {
        y -= 14.0;
        for line in wrap(&doc.outro, 10.0, false, RIGHT - LEFT) {
            p.text(LEFT, y, 10.0, false, TEXT, Align::Left, &line);
            y -= 13.5;
        }
        y -= 30.0;
        p.line(LEFT, y, LEFT + 180.0, y, 0.6, TEXT);
        p.text(LEFT, y - 11.0, 8.0, false, MUTED, Align::Left, l("Datum, Unterschrift Empfänger"));
        pages.push(p);
        let n = pages.len();
        for (i, page) in pages.iter_mut().enumerate() {
            footer(page, c, i + 1, n, lang);
        }
        return (pages, format!("{kind} {number}"), c.name.clone());
    }

    // Totals; keep them together with the closing text on one page.
    let tax_rows = if small { 0 } else { totals.by_rate.keys().filter(|r| **r > 0).count() };
    let outro = wrap(&doc.outro, 10.0, false, RIGHT - LEFT);
    let pay = payment_lines(doc, small, lang);
    let needed = 30.0 + tax_rows as f32 * 14.0 + 30.0 + (outro.len() + pay.len()) as f32 * 13.5 + if small { 16.0 } else { 0.0 };
    if y - needed < BOTTOM {
        pages.push(std::mem::take(&mut p));
        y = PAGE_H - 25.0 * MM;
    }
    y -= 8.0;
    let label_x = RIGHT - 190.0;
    p.text(label_x, y, 10.0, false, TEXT, Align::Left, l("Summe netto"));
    p.text(RIGHT, y, 10.0, false, TEXT, Align::Right, &money(totals.net));
    y -= 14.0;
    if !small {
        for (rate, (_net, tax)) in &totals.by_rate {
            if *rate > 0 {
                p.text(label_x, y, 10.0, false, TEXT, Align::Left, &crate::i18n::fill(l("zzgl. Umsatzsteuer {} %"), &[rate]));
                p.text(RIGHT, y, 10.0, false, TEXT, Align::Right, &money(*tax));
                y -= 14.0;
            }
        }
    }
    p.line(label_x, y + 8.0, RIGHT, y + 8.0, 1.2, TEXT);
    y -= 6.0;
    p.text(label_x, y, 11.5, true, TEXT, Align::Left, l("Gesamtbetrag"));
    p.text(RIGHT, y, 11.5, true, TEXT, Align::Right, &money(totals.gross));
    y -= 28.0;
    if c.small_business {
        p.text(LEFT, y, 8.5, false, MUTED, Align::Left, l("Gemäß § 19 UStG wird keine Umsatzsteuer berechnet."));
        y -= 16.0;
    } else if let Some(note) = doc.vat_case.invoice_note() {
        p.text(LEFT, y, 8.5, false, TEXT, Align::Left, l(note));
        y -= 12.0;
        if let Some(k) = contact.filter(|k| !k.vat_id.is_empty()) {
            p.text(LEFT, y, 8.5, false, MUTED, Align::Left, &crate::i18n::fill(l("USt-IdNr. des Kunden: {}"), &[&k.vat_id]));
            y -= 12.0;
        }
        y -= 4.0;
    }
    let paid = doc.paid_total();
    if doc.kind.is_billing() && paid != 0 && doc.status == DocStatus::Offen {
        p.text(label_x, y, 10.0, false, TEXT, Align::Left, l("Bereits bezahlt"));
        p.text(RIGHT, y, 10.0, false, TEXT, Align::Right, &money(paid));
        y -= 14.0;
        p.text(label_x, y, 10.5, true, TEXT, Align::Left, l("Noch offen"));
        p.text(RIGHT, y, 10.5, true, TEXT, Align::Right, &money(totals.gross - paid));
        y -= 22.0;
    }
    for line in &pay {
        p.text(LEFT, y, 10.0, false, TEXT, Align::Left, line);
        y -= 13.5;
    }
    if !pay.is_empty() {
        y -= 6.0;
    }
    for line in outro {
        p.text(LEFT, y, 10.0, false, TEXT, Align::Left, &line);
        y -= 13.5;
    }
    pages.push(p);

    let n = pages.len();
    for (i, page) in pages.iter_mut().enumerate() {
        footer(page, c, i + 1, n, lang);
    }
    (pages, format!("{kind} {number}"), c.name.clone())
}

/// One block of a text report: a heading, paragraphs, or a table of label/amount rows.
pub enum Block {
    Heading(String),
    Paragraph(String),
    /// Rows of (label, value); a row with an empty label draws a rule.
    Rows(Vec<(String, String, bool)>),
}

/// A plain report on the letterhead, e.g. the procedure documentation or a balance sheet.
pub fn report_pdf(data: &Data, title: &str, subtitle: &str, blocks: &[Block]) -> Vec<u8> {
    let c = &data.company;
    let mut pages = Vec::new();
    let mut p = Page::default();
    let mut y = PAGE_H - 22.0 * MM;
    p.text(LEFT, y, 14.0, true, ACCENT, Align::Left, &c.name);
    p.rect(LEFT, y - 7.0, RIGHT - LEFT, 1.5, ACCENT);
    y -= 34.0;
    p.text(LEFT, y, 16.0, true, TEXT, Align::Left, title);
    y -= 16.0;
    p.text(LEFT, y, 9.5, false, MUTED, Align::Left, subtitle);
    y -= 22.0;
    let mut new_page = |p: &mut Page, y: &mut f32| {
        if *y < BOTTOM {
            pages.push(std::mem::take(p));
            *y = PAGE_H - 25.0 * MM;
        }
    };
    for b in blocks {
        match b {
            Block::Heading(h) => {
                if y - 40.0 < BOTTOM {
                    y = BOTTOM - 1.0;
                    new_page(&mut p, &mut y);
                }
                y -= 6.0;
                p.text(LEFT, y, 11.5, true, ACCENT, Align::Left, h);
                y -= 16.0;
            }
            Block::Paragraph(text) => {
                for para in text.split('\n') {
                    for line in wrap(para, 9.5, false, RIGHT - LEFT) {
                        new_page(&mut p, &mut y);
                        p.text(LEFT, y, 9.5, false, TEXT, Align::Left, &line);
                        y -= 12.5;
                    }
                }
                y -= 6.0;
            }
            Block::Rows(rows) => {
                for (label, value, bold) in rows {
                    new_page(&mut p, &mut y);
                    if label.is_empty() && value.is_empty() {
                        p.line(LEFT, y + 8.0, RIGHT, y + 8.0, 0.5, RULE);
                        y -= 4.0;
                        continue;
                    }
                    for (k, line) in wrap(label, 9.5, *bold, RIGHT - LEFT - 120.0).iter().enumerate() {
                        p.text(LEFT, y - k as f32 * 12.0, 9.5, *bold, TEXT, Align::Left, line);
                    }
                    p.text(RIGHT, y, 9.5, *bold, TEXT, Align::Right, value);
                    y -= 14.0;
                }
                y -= 6.0;
            }
        }
    }
    pages.push(p);
    let n = pages.len();
    for (i, page) in pages.iter_mut().enumerate() {
        footer(page, c, i + 1, n, Lang::De);
    }
    write_pdf(&pages, title, &c.name)
}

/// Decodes standard base64 (used for the stored logo).
pub fn decode_base64(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut buf, mut bits) = (0u32, 0);
    for v in s.bytes().filter_map(val) {
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    out
}

pub fn encode_base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(A[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Turns RGB pixels into a logo for the letterhead.
pub fn make_logo(width: u32, height: u32, rgb: &[u8]) -> Logo {
    Logo { width, height, rgb: encode_base64(&deflate(rgb)) }
}

/// A file name like "Rechnung RE-2026-0012.pdf".
pub fn file_name(doc: &Document) -> String {
    let number = if doc.number.is_empty() { "Entwurf".to_string() } else { doc.number.clone() };
    let clean: String = number
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    format!("{} {clean}.pdf", doc.kind.label())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_and_encoding() {
        assert!((text_width("Hallo", 10.0, false) - 22.78).abs() < 0.01);
        assert_eq!(win_ansi('ä'), 0xE4);
        assert_eq!(win_ansi('€'), 0x80);
        assert_eq!(pdf_string("(a)"), b"(\\(a\\))".to_vec());
    }

    #[test]
    fn wrapping_respects_width() {
        let lines = wrap("Ein sehr langer Positionstext für die Rechnung", 10.0, false, 80.0);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| text_width(l, 10.0, false) <= 80.0));
    }

    #[test]
    fn sample_invoice_is_a_pdf() {
        let data = sample_data();
        let offer = data.documents.iter().find(|d| d.kind == DocKind::Angebot).unwrap();
        let bytes = document_pdf(&data, offer);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        // A long invoice runs over several pages.
        let mut long = offer.clone();
        long.lines = (0..45)
            .map(|i| Line { text: format!("Leistung Nummer {i} mit ausführlicher Beschreibung für Ärzte & Bäckereien"), ..offer.lines[0].clone() })
            .collect();
        let long_pdf = document_pdf(&data, &long);
        assert!(!String::from_utf8_lossy(&long_pdf).contains("/Count 1 "));
        // Set OPENKONTOR_PDF_OUT to a folder to look at the results.
        // A contact with English documents gets an English PDF.
        let mut english = data.clone();
        let cid = offer.contact_id.unwrap();
        english.contacts.iter_mut().find(|c| c.id == cid).unwrap().language = Lang::En;
        let en_pdf = document_pdf(&english, offer);
        let en_text = String::from_utf8_lossy(&en_pdf);
        assert!(en_text.contains("(Total amount)") && en_text.contains("(Quote no.)"));
        // With a logo the image is embedded once.
        let mut with_logo = data.clone();
        with_logo.company.logo = Some(make_logo(2, 1, &[255, 0, 0, 0, 0, 255]));
        let logo_pdf = String::from_utf8_lossy(&document_pdf(&with_logo, offer)).into_owned();
        assert!(logo_pdf.contains("/Subtype /Image /Width 2 /Height 1") && logo_pdf.contains("/Im1 Do"));
        assert_eq!(decode_base64(&encode_base64(b"Hallo Welt")), b"Hallo Welt");
        // A delivery note has no prices.
        let mut ls = offer.clone();
        ls.kind = DocKind::Lieferschein;
        assert!(!String::from_utf8_lossy(&document_pdf(&data, &ls)).contains("(Gesamtbetrag)"));
        if let Ok(dir) = std::env::var("OPENKONTOR_PDF_OUT") {
            std::fs::write(format!("{dir}/angebot-en.pdf"), &en_pdf).unwrap();
            std::fs::write(format!("{dir}/angebot.pdf"), &bytes).unwrap();
            std::fs::write(format!("{dir}/lang.pdf"), &long_pdf).unwrap();
        }
    }
}
