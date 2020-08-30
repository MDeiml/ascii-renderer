use ab_glyph::{Font, ScaleFont};

fn load_all_glyphs<F: Font>(unscaled_font: &F, height: u32) -> (u32, Vec<Option<Box<[u8]>>>) {
    let font = unscaled_font.into_scaled(height as f32);
    let width = font.h_advance(font.glyph_id(' ')) as u32;
    let y_offset = font.ascent();

    let glyphs = (0..font.glyph_count())
        .map(|i| {
            if i == font.glyph_id(' ').0 as usize {
                return Some(vec![0; (width * height) as usize].into_boxed_slice());
            }
            let glyph =
                ab_glyph::GlyphId(i as u16).with_scale_and_position(font.scale(), (0.0, y_offset));

            font.outline_glyph(glyph).map(|outline| {
                let mut image = vec![0; (width * height) as usize].into_boxed_slice();
                let bounds = outline.px_bounds();
                let offset_x = bounds.min.x as u32;
                let offset_y = bounds.min.y as u32;

                outline.draw(|x, y, c| {
                    let i = (x + offset_x).min(width - 1) + (y + offset_y).min(height - 1) * width;
                    image[i as usize] = (c * 255.0) as u8;
                });
                image
            })
        })
        .collect();

    (width, glyphs)
}

fn read_image<P: AsRef<std::path::Path>>(path: P) -> (u32, u32, Box<[u8]>) {
    let file = std::fs::File::open(path).unwrap();
    let decoder = png::Decoder::new(file);
    let (info, mut reader) = decoder.read_info().unwrap();
    let mut buf = vec![0; info.buffer_size()];
    reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    let image = match info.color_type {
        png::ColorType::RGBA => {
            let mut image = vec![0; (info.width * info.height) as usize];
            assert_eq!(info.buffer_size(), (info.width * info.height * 4) as usize);
            assert_eq!(info.color_type, png::ColorType::RGBA);
            for (i, pixel) in image.iter_mut().enumerate() {
                let sum = buf[4 * i] as f32 + buf[4 * i + 1] as f32 + buf[4 * i + 2] as f32;
                *pixel = (sum * buf[4 * i + 3] as f32 / (255.0 * 3.0)) as u8;
            }
            image.into_boxed_slice()
        }
        png::ColorType::Grayscale => buf.into_boxed_slice(),
        _ => unimplemented!(),
    };
    (info.width, info.height, image)
}

fn write_image<P: AsRef<std::path::Path>>(path: P, image: &[u8], width: u32, height: u32) {
    let file = std::fs::File::create(path).unwrap();
    let ref mut w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(image).unwrap();
}

fn extract_cells(
    image: &[u8],
    width: u32,
    height: u32,
    glyph_width: u32,
    glyph_height: u32,
) -> Vec<(f32, f32, f32)> {
    let num_glyphs_x = width / glyph_width;
    let num_glyphs_y = height / glyph_height;
    let mut ret = Vec::with_capacity((num_glyphs_x * num_glyphs_y) as usize);
    let w = width as usize;
    for y in 0..num_glyphs_y {
        for x in 0..num_glyphs_x {
            let mut edge_x = 0.0;
            let mut edge_y = 0.0;
            let mut brightness = 0.0;
            for dy in 0..glyph_height {
                for dx in 0..glyph_width {
                    let x0 = x * glyph_width + dx;
                    let y0 = y * glyph_height + dy;
                    let i = (x0 + y0 * width) as usize;
                    if x0 != 0 && x0 != width - 1 && y0 != 0 && y0 != height - 1 {
                        let mut ex = 0.0;
                        let mut ey = 0.0;
                        ex += image[i - 1 - w] as f32 / 255.0;
                        ex += 2.0 * image[i - 1] as f32 / 255.0;
                        ex += image[i - 1 + w] as f32 / 255.0;
                        ex -= image[i + 1 - w] as f32 / 255.0;
                        ex -= 2.0 * image[i + 1] as f32 / 255.0;
                        ex -= image[i + 1 + w] as f32 / 255.0;
                        ey += image[i - 1 - w] as f32 / 255.0;
                        ey += 2.0 * image[i - w] as f32 / 255.0;
                        ey += image[i + 1 - w] as f32 / 255.0;
                        ey -= image[i - 1 + w] as f32 / 255.0;
                        ey -= 2.0 * image[i + w] as f32 / 255.0;
                        ey -= image[i + 1 + w] as f32 / 255.0;
                        edge_x += ex.abs();
                        edge_y += ey.abs();
                    }
                    brightness += image[i] as f32 / 255.0;
                }
            }
            ret.push((brightness, edge_x, edge_y));
        }
    }
    ret
}

fn normalize_cells_quantile(cells: &mut [(f32, f32, f32)]) {
    let mut sorted = cells.iter().copied().enumerate().collect::<Vec<_>>();
    sorted.sort_by(|(_, (b1, _, _)), (_, (b2, _, _))| b1.partial_cmp(b2).unwrap());
    for (n, (i, _)) in sorted.iter().enumerate() {
        cells[*i].0 = n as f32 / cells.len() as f32;
    }

    sorted.sort_by(|(_, (_, ex1, _)), (_, (_, ex2, _))| ex1.partial_cmp(ex2).unwrap());
    for (n, (i, _)) in sorted.iter().enumerate() {
        cells[*i].1 = n as f32 / cells.len() as f32;
    }

    sorted.sort_by(|(_, (_, _, ey1)), (_, (_, _, ey2))| ey1.partial_cmp(ey2).unwrap());
    for (n, (i, _)) in sorted.iter().enumerate() {
        cells[*i].2 = n as f32 / cells.len() as f32;
    }
}

fn normalize_cells(cells: &mut [(f32, f32, f32)]) {
    let (b_min, b_max, ex_min, ex_max, ey_min, ey_max) =
        cells.iter().filter(|(b, _, _)| *b != 0.0).fold(
            (
                f32::INFINITY,
                0f32,
                f32::INFINITY,
                0f32,
                f32::INFINITY,
                0f32,
            ),
            |(b_min, b_max, ex_min, ex_max, ey_min, ey_max), (b, ex, ey)| {
                (
                    b_min.min(*b),
                    b_max.max(*b),
                    ex_min.min(*ex),
                    ex_max.max(*ex),
                    ey_min.min(*ey),
                    ey_max.max(*ey),
                )
            },
        );
    for (b, ex, ey) in cells.iter_mut() {
        if *b == 0.0 {
            continue;
        }
        *b -= b_min;
        *b /= b_max - b_min;
        *ex -= ex_min;
        *ex /= ex_max - ex_min;
        *ey -= ey_min;
        *ey /= ey_max - ey_min;
    }
}

fn main() {
    let glyph_height = 20u32;
    let font = ab_glyph::FontRef::try_from_slice(include_bytes!("../DejaVuSansMono.ttf")).unwrap();
    let (glyph_width, glyphs) = load_all_glyphs(&font, glyph_height);
    let glyphs = glyphs
        .into_iter()
        .skip(1)
        .take(98)
        .filter_map(|x| x)
        .collect::<Vec<_>>();
    let (width, height, image) = read_image("mandelbulb.png");
    let num_glyphs_x = width / glyph_width;
    let num_glyphs_y = height / glyph_height;

    let mut image_cells = extract_cells(&image, width, height, glyph_width, glyph_height);
    let mut glyph_cells = glyphs
        .iter()
        .map(|g| {
            extract_cells(&g, glyph_width, glyph_height, glyph_width, glyph_height)
                .pop()
                .unwrap()
        })
        .collect::<Vec<_>>();
    normalize_cells(&mut image_cells);
    normalize_cells_quantile(&mut glyph_cells);
    let mut output = vec![0; (width * height) as usize];

    for y in 0..num_glyphs_y {
        for x in 0..num_glyphs_x {
            let mut best_match = 0;
            let mut best_error = f32::INFINITY;
            let (ib, iex, iey) = image_cells[(x + y * num_glyphs_x) as usize];
            for (i, (gb, gex, gey)) in glyph_cells.iter().enumerate() {
                let error = (gb - ib).powi(2) * 0.05 + (iex - gex).powi(2) + (iey - gey).powi(2);
                if error < best_error {
                    best_error = error;
                    best_match = i;
                }
            }
            let glyph = &glyphs[best_match];
            for dy in 0..glyph_height {
                for dx in 0..glyph_width {
                    let output_index = (x * glyph_width + dx) + (y * glyph_height + dy) * width;
                    let glyph_index = dx + dy * glyph_width;
                    output[output_index as usize] = glyph[glyph_index as usize];
                }
            }
        }
    }

    write_image("output.png", &output, width, height);
}
