use super::*;

/// Encodes original row and column detail into a synthetic PNG.
fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_fn(width, height, |x, y| {
        image::Rgba([x as u8, y as u8, (y / width) as u8, 255])
    });
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png).unwrap();
    bytes.into_inner()
}

#[test]
fn large_strips_and_particle_atlases_retain_every_source_pixel() {
    let policy = TexturePolicy::for_device(assets::texture::policy::MAX_TEXTURE_SIDE);
    for (width, height, strip) in [
        (64, 2048, true),
        (128, 4096, true),
        (64, 8384, true),
        (2048, 2048, false),
    ] {
        let bytes = png(width, height);
        let view =
            LayeredPackView::new(crate::test_support::stack(&[("textures/test.png", &bytes)]));
        let texture =
            decode_pack_texture_with_policy(&view, "textures/test", policy, strip).unwrap();
        assert_eq!([texture.width, texture.height], [width, height]);
        assert_eq!(texture.rgba8.len(), (width * height * 4) as usize);
        let x = width - 1;
        let y = height - 1;
        assert_eq!(
            &texture.rgba8[((y * width + x) * 4) as usize..][..4],
            &[x as u8, y as u8, (y / width) as u8, 255]
        );
        assert!(decode_pack_texture(&view, "textures/test").is_none());
    }
}

#[test]
fn horizontal_strip_decoding_checks_both_axes_before_pixel_allocation() {
    let policy = TexturePolicy::for_device(64);
    for (width, height) in [(8192, 64), (64, 8192)] {
        let bytes = png(width, height);
        let view =
            LayeredPackView::new(crate::test_support::stack(&[("textures/test.png", &bytes)]));
        let texture =
            decode_pack_texture_with_policy(&view, "textures/test", policy, true).unwrap();
        assert_eq!([texture.width, texture.height], [width, height]);
        assert_eq!(texture.rgba8.len(), (width * height * 4) as usize);
        assert_eq!(
            &texture.rgba8[texture.rgba8.len() - 4..],
            &[
                (width - 1) as u8,
                (height - 1) as u8,
                ((height - 1) / width) as u8,
                255
            ]
        );
        assert!(decode_pack_texture_with_policy(&view, "textures/test", policy, false).is_none());
    }
    for (width, height) in [(8193, 64), (8192, 65)] {
        assert!(
            decode_image_with_policy(&png(width, height), ImageFormat::Png, policy, true).is_none()
        );
    }
}

#[test]
fn malformed_over_budget_and_device_oversized_images_are_rejected() {
    let policy = TexturePolicy::for_device(assets::texture::policy::MAX_TEXTURE_SIDE);
    assert!(decode_image_with_policy(b"broken", ImageFormat::Png, policy, true).is_none());
    assert!(decode_image_with_policy(&png(2048, 4096), ImageFormat::Png, policy, true).is_none());
    assert!(decode_image_with_policy(&png(64, 8385), ImageFormat::Png, policy, true).is_none());
    assert!(
        decode_image_with_policy(
            &png(2048, 2048),
            ImageFormat::Png,
            TexturePolicy::for_device(1024),
            false
        )
        .is_none()
    );
}

#[test]
fn first_strip_frame_keeps_rows_and_columns_on_both_axes() {
    let policy = TexturePolicy::for_device(64);
    for (width, height) in [(8192, 64), (64, 8192), (64, 64), (96, 64)] {
        let mut texture = decode_image_with_policy(
            &png(width, height),
            ImageFormat::Png,
            TexturePolicy::for_device(128),
            true,
        )
        .unwrap();
        let original = texture.clone();
        texture.retain_first_strip_frame();
        if (width, height) == (96, 64) {
            assert_eq!(texture, original);
            continue;
        }
        assert_eq!([texture.width, texture.height], [64; 2]);
        assert_eq!(
            policy.source_bytes(texture.width, texture.height, false),
            Some(texture.rgba8.len())
        );
        for y in 0..64usize {
            for x in 0..64usize {
                assert_eq!(
                    &texture.rgba8[(y * 64 + x) * 4..][..4],
                    &[x as u8, y as u8, 0, 255]
                );
            }
        }
    }
}

#[test]
fn first_strip_frame_leaves_malformed_buffers_unchanged() {
    for (width, height) in [(0, 16), (16, 0), (16, 32), (32, 16), (u32::MAX, 1)] {
        let mut texture = DecodedTexture {
            width,
            height,
            rgba8: vec![7; 4].into(),
        };
        let original = texture.clone();
        texture.retain_first_strip_frame();
        assert_eq!(texture, original);
    }
}
