use super::*;

fn material() -> Vec<u8> {
    let names = b"me_colour\0me_count\0me_mapHeapIndex\0";
    let n = 3;
    let mut b = Vec::new();
    for v in [n, 1, 20, 20, 20, 0, 0, 0] {
        b.extend_from_slice(&(v as u32).to_le_bytes());
    }
    let records: [[u32; 9]; 3] = [
        [0, 1, 0x30003, 1, 12, 0, 0, 0, 0],
        [10, 1, 6, 1, 4, 0, 12, 12, 0],
        [19, 1, 6, 3, 4, 0, 16, 16, 0],
    ];
    for r in records {
        for v in r {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    for v in [1.0f32, 1.0, 1.0] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&7u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(names);
    b
}

#[test]
fn sets_a_float_vector_and_a_uint_in_place() {
    let mut m = material();
    let len = m.len();
    assert_eq!(get(&m, "me_colour").unwrap(), [1.0, 1.0, 1.0]);
    set(&mut m, "me_colour", &[0.25, 1.0, 0.5]).unwrap();
    set(&mut m, "me_count", &[3.0]).unwrap();
    assert_eq!(get(&m, "me_colour").unwrap(), [0.25, 1.0, 0.5]);
    assert_eq!(get(&m, "me_count").unwrap(), [3.0]);
    assert_eq!(m.len(), len);
}

#[test]
fn refuses_unknown_names_heap_indices_and_wrong_widths() {
    let mut m = material();
    assert_eq!(
        set(&mut m, "me_nope", &[1.0]),
        Err(Error::NoConstant("me_nope".into()))
    );
    assert_eq!(
        set(&mut m, "me_mapHeapIndex", &[1.0]),
        Err(Error::NotAValue("me_mapHeapIndex".into()))
    );
    assert_eq!(
        set(&mut m, "me_colour", &[1.0]),
        Err(Error::Width {
            name: "me_colour".into(),
            want: 3,
            got: 1
        })
    );
    assert_eq!(constants(&m[..10]), Err(Error::Truncated));
}

fn full() -> Material {
    let tex = "eaid://ui_package/art/a.tif";
    let p = |name: &str, value, ty, class, cbuffer, offset, link: Option<&str>| Param {
        name: name.into(),
        value,
        ty,
        class,
        cbuffer,
        offset,
        link: link.map(Into::into),
    };
    Material {
        shader: "eaid://shader_package/x/y.nefx2".into(),
        params: vec![
            p(
                "me_colour",
                ParamValue::Bytes(
                    [1.0f32, 1.0, 1.0, 1.0]
                        .iter()
                        .flat_map(|f| f.to_le_bytes())
                        .collect(),
                ),
                0x40003,
                1,
                0,
                0,
                None,
            ),
            p(
                "me_diffuseTexHeapIndex",
                ParamValue::Bytes(vec![0; 4]),
                6,
                3,
                0,
                16,
                Some(tex),
            ),
            p(
                "me_diffuseTex",
                ParamValue::Texture(tex.into()),
                2,
                1,
                0xffff_ffff,
                0,
                None,
            ),
        ],
        cbuffers: vec![("PerMaterial".into(), 20)],
    }
}

#[test]
fn a_whole_material_round_trips_and_links_a_texture() {
    let mut m = full();
    let bytes = m.to_bytes();
    assert_eq!(Material::parse(&bytes).unwrap(), m);
    m.link_texture("me_diffuseTex", "eaid://ui_package/art/b.tif")
        .unwrap();
    m.set_floats("me_colour", &[0.5, 0.25, 1.0, 1.0]).unwrap();
    let back = Material::parse(&m.to_bytes()).unwrap();
    assert_eq!(back.texture("me_diffuseTex"), Some("eaid://ui_package/art/b.tif"));
    assert_eq!(
        back.param("me_diffuseTexHeapIndex").unwrap().link.as_deref(),
        Some("eaid://ui_package/art/b.tif")
    );
    assert_eq!(get(&back.to_bytes(), "me_colour").unwrap(), [0.5, 0.25, 1.0, 1.0]);
}

#[test]
fn linking_needs_the_slot_and_its_heap_index() {
    let mut m = full();
    assert_eq!(
        m.link_texture("me_nope", "x"),
        Err(Error::NoParam("me_nopeHeapIndex".into()))
    );
    m.params.remove(2);
    assert_eq!(
        m.link_texture("me_diffuseTex", "x"),
        Err(Error::NoParam("me_diffuseTex".into()))
    );
    assert!(Material::parse(&full().to_bytes()[..40]).is_err());
}
