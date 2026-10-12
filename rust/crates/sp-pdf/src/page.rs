//! Page-level facts: the view box, rotation, resources and content bytes,
//! read the way pdf.js's `Page` reads them.
//!
//! Ported from pdf.js `src/core/document.js` and `src/display/display_utils.js`
//! (`PageViewport`), Mozilla Foundation, Apache License 2.0.

use std::collections::HashSet;
use std::rc::Rc;

use crate::obj::{Dict, Obj, Ref, XRef};

pub const LETTER_SIZE_MEDIABOX: [f64; 4] = [0.0, 0.0, 612.0, 792.0];

pub struct PageInfo {
    pub dict: Rc<Dict>,
    pub view: [f64; 4],
    pub rotate: i64,
    pub resources: Rc<Dict>,
}

/// `getInheritableProperty` with `stopWhenFound: false`: every value up
/// the Parent chain, nearest first.
fn inheritable(xref: &XRef<'_>, page: &Rc<Dict>, key: &str, get_array: bool) -> Vec<Obj> {
    let mut values = Vec::new();
    let mut visited: HashSet<Ref> = HashSet::new();
    let mut dict = Some(page.clone());
    let mut guard = 0;
    while let Some(d) = dict {
        guard += 1;
        if guard > 1000 {
            break;
        }
        if let Some(id) = d.obj_id {
            if visited.contains(&id) {
                break;
            }
            visited.insert(id);
        }
        let v = if get_array {
            match xref.get(&d, key) {
                Obj::Array(a) => Obj::Array(Rc::new(a.iter().map(|o| xref.resolve(o)).collect())),
                o => o,
            }
        } else {
            xref.get(&d, key)
        };
        if d.has(key) {
            values.push(v);
        }
        dict = xref.get(&d, "Parent").dict().cloned();
    }
    values
}

fn lookup_normal_rect(o: &Obj) -> Option<[f64; 4]> {
    let a = o.array()?;
    if a.len() != 4 {
        return None;
    }
    let v: Vec<f64> = a.iter().map(|x| x.num()).collect::<Option<Vec<f64>>>()?;
    let mut r = [v[0], v[1], v[2], v[3]];
    if v[0] > v[2] {
        r[0] = v[2];
        r[2] = v[0];
    }
    if v[1] > v[3] {
        r[1] = v[3];
        r[3] = v[1];
    }
    Some(r)
}

fn bounding_box(xref: &XRef<'_>, page: &Rc<Dict>, name: &str) -> Option<[f64; 4]> {
    let vals = inheritable(xref, page, name, true);
    let first = vals.first()?;
    let b = lookup_normal_rect(first)?;
    if b[2] - b[0] > 0.0 && b[3] - b[1] > 0.0 {
        Some(b)
    } else {
        None
    }
}

/// `Dict.merge` without sub-dict merging: the first value per key wins.
fn merge(dicts: &[Rc<Dict>]) -> Rc<Dict> {
    let mut out = Dict::default();
    for d in dicts {
        for (k, v) in &d.entries {
            if !out.has(k) {
                out.set(k, v.clone());
            }
        }
    }
    Rc::new(out)
}

pub fn page_info(xref: &XRef<'_>, page: Rc<Dict>) -> PageInfo {
    let media_box = bounding_box(xref, &page, "MediaBox").unwrap_or(LETTER_SIZE_MEDIABOX);
    let crop_box = bounding_box(xref, &page, "CropBox");
    let mut view = media_box;
    if let Some(cb) = crop_box
        && cb != media_box
    {
        // Util.intersect
        let x_low = cb[0].min(cb[2]).max(media_box[0].min(media_box[2]));
        let x_high = cb[0].max(cb[2]).min(media_box[0].max(media_box[2]));
        let y_low = cb[1].min(cb[3]).max(media_box[1].min(media_box[3]));
        let y_high = cb[1].max(cb[3]).min(media_box[1].max(media_box[3]));
        if x_low <= x_high && y_low <= y_high && x_high - x_low > 0.0 && y_high - y_low > 0.0 {
            view = [x_low, y_low, x_high, y_high];
        }
    }
    let rot_vals = inheritable(xref, &page, "Rotate", false);
    let mut rotate = match rot_vals.first() {
        Some(Obj::Num(n)) if *n != 0.0 => *n,
        _ => 0.0,
    };
    if rotate % 90.0 != 0.0 || rotate.is_nan() {
        rotate = 0.0;
    } else if rotate >= 360.0 {
        rotate %= 360.0;
    } else if rotate < 0.0 {
        rotate = (rotate % 360.0 + 360.0) % 360.0;
    }
    let res_vals = inheritable(xref, &page, "Resources", false);
    let resources = match res_vals.len() {
        0 => Rc::new(Dict::default()),
        _ if res_vals.len() == 1 || res_vals[0].dict().is_none() => match &res_vals[0] {
            Obj::Dict(d) => d.clone(),
            _ => Rc::new(Dict::default()),
        },
        _ => {
            let ds: Vec<Rc<Dict>> = res_vals.iter().filter_map(|o| o.dict().cloned()).collect();
            merge(&ds)
        }
    };
    PageInfo {
        dict: page,
        view,
        rotate: rotate as i64,
        resources,
    }
}

fn is_image_stream(xref: &XRef<'_>, s: &crate::obj::Stream) -> bool {
    let last = match xref.get2(&s.dict, "F", "Filter") {
        Obj::Name(n) => Some(n.to_string()),
        Obj::Array(a) => a
            .last()
            .and_then(|o| xref.resolve(o).name().map(str::to_string)),
        _ => None,
    };
    matches!(
        last.as_deref(),
        Some("DCTDecode" | "DCT" | "JPXDecode" | "JPX" | "JBIG2Decode" | "CCITTFaxDecode" | "CCF")
    )
}

/// The page's content, its streams joined with nothing between them, and
/// the content stream's own dictionary (for its rare /Resources).
pub fn content_bytes(xref: &XRef<'_>, page: &Dict) -> (Vec<u8>, Option<Rc<Dict>>) {
    match xref.get(page, "Contents") {
        Obj::Stream(s) => {
            if is_image_stream(xref, &s) {
                return (Vec::new(), None);
            }
            (
                xref.stream_bytes(&s).unwrap_or_default(),
                Some(s.dict.clone()),
            )
        }
        Obj::Array(a) => {
            let mut out = Vec::new();
            for o in a.iter() {
                if let Obj::Stream(s) = xref.resolve(o)
                    && !is_image_stream(xref, &s)
                    && let Some(b) = xref.stream_bytes(&s)
                {
                    out.extend_from_slice(&b);
                }
            }
            (out, None)
        }
        _ => (Vec::new(), None),
    }
}

/// `#getMergedResources`: a content stream's own /Resources over the page's.
pub fn merged_resources(
    xref: &XRef<'_>,
    stream_dict: Option<&Rc<Dict>>,
    page_res: &Rc<Dict>,
) -> Rc<Dict> {
    let Some(sd) = stream_dict else {
        return page_res.clone();
    };
    let local = xref.get(sd, "Resources");
    let Some(local) = local.dict() else {
        return page_res.clone();
    };
    if local.entries.is_empty() {
        return page_res.clone();
    }
    // mergeSubDicts: a key present as a dictionary in both is merged.
    let mut out = Dict::default();
    let dicts = [local.clone(), page_res.clone()];
    let mut keys: Vec<Rc<str>> = Vec::new();
    for d in &dicts {
        for (k, _) in &d.entries {
            if !keys.contains(k) {
                keys.push(k.clone());
            }
        }
    }
    for k in keys {
        let vals: Vec<&Obj> = dicts.iter().filter_map(|d| d.get_raw(&k)).collect();
        let first_is_dict = matches!(vals.first(), Some(Obj::Dict(_)));
        if vals.len() == 1 || !first_is_dict {
            out.set(&k, vals[0].clone());
            continue;
        }
        let mut sub = Dict::default();
        for v in vals {
            if let Obj::Dict(d) = v {
                for (kk, vv) in &d.entries {
                    if !sub.has(kk) {
                        sub.set(kk, vv.clone());
                    }
                }
            }
        }
        if !sub.entries.is_empty() {
            out.set(&k, Obj::Dict(Rc::new(sub)));
        }
    }
    Rc::new(out)
}

/// `page.getViewport({ scale: 1 })`'s width and height.
pub fn viewport_size(view: &[f64; 4], rotate: i64) -> (f64, f64) {
    let mut rotation = rotate % 360;
    if rotation < 0 {
        rotation += 360;
    }
    if rotation == 90 || rotation == 270 {
        (view[3] - view[1], view[2] - view[0])
    } else {
        (view[2] - view[0], view[3] - view[1])
    }
}
