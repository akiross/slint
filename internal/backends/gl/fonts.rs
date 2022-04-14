// Copyright © SixtyFPS GmbH <info@slint-ui.com>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-commercial

// cspell:ignore Noto fontconfig

use femtovg::TextContext;
use i_slint_core::graphics::{FontRequest, Point, Size};
use i_slint_core::items::{TextHorizontalAlignment, TextOverflow, TextVerticalAlignment, TextWrap};
use i_slint_core::textlayout::{Glyph, TextLayout, TextShaper};
use i_slint_core::{SharedString, SharedVector};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

pub const DEFAULT_FONT_SIZE: f32 = 12.;
pub const DEFAULT_FONT_WEIGHT: i32 = 400; // CSS normal

#[cfg(not(any(
    target_family = "windows",
    target_os = "macos",
    target_os = "ios",
    target_arch = "wasm32"
)))]
mod fontconfig;

/// This function can be used to register a custom TrueType font with Slint,
/// for use with the `font-family` property. The provided slice must be a valid TrueType
/// font.
pub fn register_font_from_memory(data: &'static [u8]) -> Result<(), Box<dyn std::error::Error>> {
    FONT_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .available_fonts
            .load_font_source(fontdb::Source::Binary(std::sync::Arc::new(data)))
    });
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn register_font_from_path(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let requested_path = path.canonicalize().unwrap_or_else(|_| path.to_owned());
    FONT_CACHE.with(|cache| {
        for face_info in cache.borrow().available_fonts.faces() {
            match &face_info.source {
                fontdb::Source::Binary(_) => {}
                fontdb::Source::File(loaded_path) | fontdb::Source::SharedFile(loaded_path, ..) => {
                    if *loaded_path == requested_path {
                        return Ok(());
                    }
                }
            }
        }

        cache.borrow_mut().available_fonts.load_font_file(requested_path).map_err(|e| e.into())
    })
}

#[cfg(target_arch = "wasm32")]
pub fn register_font_from_path(_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    return Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Registering fonts from paths is not supported in WASM builds",
    )
    .into());
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct FontCacheKey {
    family: SharedString,
    weight: i32,
}

pub struct Font {
    fonts: SharedVector<ScaledFont>,
    pub(crate) pixel_size: f32,
    //text_context: TextContext,
}

impl Font {
    pub fn init_paint(&self, letter_spacing: f32, mut paint: femtovg::Paint) -> femtovg::Paint {
        // (femtovg takes max 8 from the slice)
        let mut femtovg_fonts = [self.fonts.first().unwrap().femtovg_font_id; 8];
        for (i, loaded_font) in self.fonts.iter().enumerate() {
            femtovg_fonts[i] = loaded_font.femtovg_font_id;
        }
        paint.set_font(&femtovg_fonts);
        paint.set_font_size(self.pixel_size);
        paint.set_text_baseline(femtovg::Baseline::Top);
        paint.set_letter_spacing(letter_spacing);
        paint
    }

    pub fn text_size(
        &self,
        letter_spacing: Option<f32>,
        text: &str,
        max_width: Option<f32>,
    ) -> Size {
        let layout = TextLayout { font: self, letter_spacing };
        let (longest_line_width, height) = layout.text_size(text, max_width);
        euclid::size2(longest_line_width, height)
    }
}

impl TextShaper for Font {
    type LengthPrimitive = f32;
    type Length = f32;
    type PlatformGlyphData = PlatformGlyph;

    fn shape_text<
        GlyphStorage: core::iter::Extend<Glyph<Self::Length, Self::PlatformGlyphData>>
            + core::convert::AsRef<[Glyph<Self::Length, Self::PlatformGlyphData>]>
            + core::ops::Index<usize, Output = Glyph<Self::Length, Self::PlatformGlyphData>>
            + core::ops::IndexMut<usize, Output = Glyph<Self::Length, Self::PlatformGlyphData>>,
    >(
        &self,
        text: &str,
        glyphs: &mut GlyphStorage,
    ) {
        self.fonts.as_slice().shape_text(text, glyphs)
    }

    fn glyph_for_char(&self, ch: char) -> Option<Glyph<Self::Length, Self::PlatformGlyphData>> {
        self.fonts.as_slice().glyph_for_char(ch)
    }
}

impl i_slint_core::textlayout::FontMetrics<f32> for Font {
    fn ascent(&self) -> f32 {
        self.fonts.iter().map(|f| f.ascent()).reduce(f32::max).unwrap_or_default()
    }

    fn descent(&self) -> f32 {
        self.fonts.iter().map(|f| f.descent()).reduce(f32::min).unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default)]
pub struct PlatformGlyph {
    pub font_id: Option<femtovg::FontId>,
    pub glyph_id: Option<core::num::NonZeroU16>,
}

impl TextShaper for ScaledFont {
    type LengthPrimitive = f32;
    type Length = f32;
    type PlatformGlyphData = PlatformGlyph;

    fn shape_text<
        GlyphStorage: core::iter::Extend<Glyph<Self::Length, Self::PlatformGlyphData>>,
    >(
        &self,
        text: &str,
        glyphs: &mut GlyphStorage,
    ) {
        self.with_face(|face| {
            let scale = self.pixel_size / (face.units_per_em() as f32);
            let mut buffer = rustybuzz::UnicodeBuffer::new();
            buffer.push_str(text);
            let glyph_buffer = rustybuzz::shape(&face, &[], buffer);

            let output_glyph_generator =
                glyph_buffer.glyph_infos().iter().zip(glyph_buffer.glyph_positions().iter()).map(
                    |(info, position)| {
                        let mut out_glyph = Glyph::default();

                        out_glyph.platform_glyph = PlatformGlyph {
                            font_id: Some(self.femtovg_font_id),
                            glyph_id: core::num::NonZeroU16::new(info.glyph_id as u16),
                        };

                        out_glyph.offset_x = scale * position.x_offset as f32;
                        out_glyph.offset_y = scale * position.y_offset as f32;
                        out_glyph.advance = scale * position.x_advance as f32;

                        out_glyph.text_byte_offset = info.cluster as usize;

                        out_glyph
                    },
                );

            // Cannot return impl Iterator, so extend argument instead
            glyphs.extend(output_glyph_generator);
        })
    }

    fn glyph_for_char(&self, ch: char) -> Option<Glyph<Self::Length, Self::PlatformGlyphData>> {
        self.with_face(|face| {
            let scale = self.pixel_size / (face.units_per_em() as f32);

            face.glyph_index(ch).map(|glyph_id| {
                let mut out_glyph = Glyph::default();
                out_glyph.platform_glyph = PlatformGlyph {
                    font_id: Some(self.femtovg_font_id),
                    glyph_id: core::num::NonZeroU16::new(glyph_id.0),
                };
                out_glyph.offset_x = 0.;
                out_glyph.offset_y = 0.;
                out_glyph.advance = scale * face.glyph_hor_advance(glyph_id).unwrap() as f32;
                out_glyph
            })
        })
    }

    fn has_glyph_for_char(&self, ch: char) -> bool {
        if ch.is_control() || ch.is_whitespace() {
            return true;
        }
        FONT_CACHE.with(|font_cache| {
            let mut font_cache = font_cache.borrow_mut();
            font_cache.face_supports_char(self.fontdb_face_id, ch)
        })
    }
}

impl i_slint_core::textlayout::FontMetrics<f32> for ScaledFont {
    fn ascent(&self) -> f32 {
        self.ascent * self.pixel_size / self.units_per_em
    }

    fn descent(&self) -> f32 {
        self.descent * self.pixel_size / self.units_per_em
    }
}

pub(crate) fn text_size(
    font_request: &i_slint_core::graphics::FontRequest,
    scale_factor: f32,
    text: &str,
    max_width: Option<f32>,
) -> Size {
    let font =
        FONT_CACHE.with(|cache| cache.borrow_mut().font(font_request.clone(), scale_factor, text));
    font.text_size(font_request.letter_spacing, text, max_width.map(|x| x * scale_factor))
        / scale_factor
}

#[derive(Copy, Clone)]
struct LoadedFont {
    femtovg_font_id: femtovg::FontId,
    fontdb_face_id: fontdb::ID,
    ascent: f32,
    descent: f32,
    units_per_em: f32,
}

impl LoadedFont {
    fn new(
        femtovg_font_id: femtovg::FontId,
        fontdb_face_id: fontdb::ID,
        data: &SharedFontData,
        face_index: u32,
    ) -> Self {
        let face = ttf_parser::Face::from_slice(data.as_ref(), face_index)
            .expect("unable to parse previously successfully parsed ttf data");
        let ascent = face.ascender() as f32;
        let descent = face.descender() as f32;
        let units_per_em = face.units_per_em() as f32;

        Self { femtovg_font_id, fontdb_face_id, ascent, descent, units_per_em }
    }

    fn with_face<T>(&self, mut callback: impl FnMut(&rustybuzz::Face) -> T) -> T {
        FONT_CACHE.with(|cache| {
            cache
                .borrow()
                .available_fonts
                .with_face_data(self.fontdb_face_id, |face_data, face_index| {
                    let face = rustybuzz::Face::from_slice(face_data, face_index)
                        .expect("unable to parse true type font");
                    callback(&face)
                })
                .unwrap()
        })
    }
}

struct ScaledFont {
    font: LoadedFont,
    pixel_size: f32,
}

impl core::ops::Deref for ScaledFont {
    type Target = LoadedFont;

    fn deref(&self) -> &Self::Target {
        &self.font
    }
}

#[derive(Clone)]
struct SharedFontData(std::sync::Arc<dyn AsRef<[u8]>>);
impl AsRef<[u8]> for SharedFontData {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref().as_ref()
    }
}

#[derive(Default)]
struct GlyphCoverage {
    // Used to express script support for all scripts except Unknown, Common and Inherited
    // For those the detailed glyph_coverage is used instead
    supported_scripts: HashMap<unicode_script::Script, bool>,
    // Especially in characters mapped to the common script, the support varies. For example
    // '✓' and the digit '1' map to Common, but not all fonts providing digits also support the
    // check mark glyph.
    exact_glyph_coverage: HashMap<char, bool>,
}

enum GlyphCoverageCheckResult {
    Incomplete,
    Improved,
    Complete,
}

pub struct FontCache {
    loaded_fonts: HashMap<FontCacheKey, LoadedFont>,
    // for a given fontdb face id, this tells us what we've learned about the script
    // coverage of the font.
    loaded_font_coverage: HashMap<fontdb::ID, GlyphCoverage>,
    pub(crate) text_context: TextContext,
    pub(crate) available_fonts: fontdb::Database,
    available_families: HashSet<SharedString>,
    #[cfg(not(any(
        target_family = "windows",
        target_os = "macos",
        target_os = "ios",
        target_arch = "wasm32"
    )))]
    fontconfig_fallback_families: Vec<String>,
}

impl Default for FontCache {
    fn default() -> Self {
        let mut font_db = fontdb::Database::new();

        #[cfg(not(any(
            target_family = "windows",
            target_os = "macos",
            target_os = "ios",
            target_arch = "wasm32"
        )))]
        let mut fontconfig_fallback_families;

        #[cfg(target_arch = "wasm32")]
        {
            let data = include_bytes!("fonts/DejaVuSans.ttf");
            font_db.load_font_data(data.to_vec());
            font_db.set_sans_serif_family("DejaVu Sans");
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            font_db.load_system_fonts();
            #[cfg(any(
                target_family = "windows",
                target_os = "macos",
                target_os = "ios",
                target_arch = "wasm32"
            ))]
            let default_sans_serif_family = "Arial";
            #[cfg(not(any(
                target_family = "windows",
                target_os = "macos",
                target_os = "ios",
                target_arch = "wasm32"
            )))]
            let default_sans_serif_family = {
                fontconfig_fallback_families = fontconfig::find_families("sans-serif");
                fontconfig_fallback_families.remove(0)
            };
            font_db.set_sans_serif_family(default_sans_serif_family);
        }
        let available_families =
            font_db.faces().iter().map(|face_info| face_info.family.as_str().into()).collect();

        Self {
            loaded_fonts: HashMap::new(),
            loaded_font_coverage: HashMap::new(),
            text_context: Default::default(),
            available_fonts: font_db,
            available_families,
            #[cfg(not(any(
                target_family = "windows",
                target_os = "macos",
                target_os = "ios",
                target_arch = "wasm32"
            )))]
            fontconfig_fallback_families,
        }
    }
}

thread_local! {
    pub static FONT_CACHE: RefCell<FontCache> = RefCell::new(Default::default())
}

impl FontCache {
    fn load_single_font(&mut self, request: &FontRequest) -> LoadedFont {
        let text_context = self.text_context.clone();
        let cache_key = FontCacheKey {
            family: request.family.clone().unwrap_or_default(),
            weight: request.weight.unwrap(),
        };

        if let Some(loaded_font) = self.loaded_fonts.get(&cache_key) {
            return *loaded_font;
        }

        let family = request
            .family
            .as_ref()
            .map_or(fontdb::Family::SansSerif, |family| fontdb::Family::Name(family));

        //let now = std::time::Instant::now();
        let query = fontdb::Query {
            families: &[family],
            weight: fontdb::Weight(request.weight.unwrap() as u16),
            ..Default::default()
        };

        let fontdb_face_id = self
            .available_fonts
            .query(&query)
            .or_else(|| {
                // If the requested family could not be found, fall back to *some* family that must exist
                let mut fallback_query = query;
                fallback_query.families = &[fontdb::Family::SansSerif];
                self.available_fonts.query(&fallback_query)
            })
            .expect("there must be a sans-serif font face registered");

        // Safety: We map font files into memory that - while we never unmap them - may
        // theoretically get corrupted/truncated by another process and then we'll crash
        // and burn. In practice that should not happen though, font files are - at worst -
        // removed by a package manager and they unlink instead of truncate, even when
        // replacing files. Unlinking OTOH is safe and doesn't destroy the file mapping,
        // the backing file becomes an orphan in a special area of the file system. That works
        // on Unixy platforms and on Windows the default file flags prevent the deletion.
        #[cfg(not(target_arch = "wasm32"))]
        let (shared_data, face_index) = unsafe {
            self.available_fonts.make_shared_face_data(fontdb_face_id).expect("unable to mmap font")
        };
        #[cfg(target_arch = "wasm32")]
        let (shared_data, face_index) = self
            .available_fonts
            .face_source(fontdb_face_id)
            .map(|(source, face_index)| {
                (
                    match source {
                        fontdb::Source::Binary(data) => data.clone(),
                        // We feed only Source::Binary into fontdb on wasm
                        #[allow(unreachable_patterns)]
                        _ => unreachable!(),
                    },
                    face_index,
                )
            })
            .expect("invalid fontdb face id");

        let shared_font_data = SharedFontData(shared_data);

        let femtovg_font_id =
            text_context.add_shared_font_with_index(shared_font_data.clone(), face_index).unwrap();

        //println!("Loaded {:#?} in {}ms.", request, now.elapsed().as_millis());
        let new_font =
            LoadedFont::new(femtovg_font_id, fontdb_face_id, &shared_font_data, face_index);
        self.loaded_fonts.insert(cache_key, new_font);
        new_font
    }

    pub fn font(
        &mut self,
        mut request: FontRequest,
        scale_factor: f32,
        reference_text: &str,
    ) -> Font {
        request.pixel_size = Some(request.pixel_size.unwrap_or(DEFAULT_FONT_SIZE) * scale_factor);
        request.weight = request.weight.or(Some(DEFAULT_FONT_WEIGHT));

        let primary_font = self.load_single_font(&request);

        use unicode_script::{Script, UnicodeScript};
        // map from required script to sample character
        let mut scripts_required: HashMap<unicode_script::Script, char> = Default::default();
        let mut chars_required: HashSet<char> = Default::default();
        for ch in reference_text.chars() {
            if ch.is_control() || ch.is_whitespace() {
                continue;
            }
            let script = ch.script();
            if script == Script::Common || script == Script::Inherited || script == Script::Unknown
            {
                chars_required.insert(ch);
            } else {
                scripts_required.insert(script, ch);
            }
        }

        let mut coverage_result = self.check_and_update_script_coverage(
            &mut scripts_required,
            &mut chars_required,
            primary_font.fontdb_face_id,
        );

        //eprintln!(
        //    "coverage for {} after checking primary font: {:#?}",
        //    reference_text, scripts_required
        //);

        let fallbacks = if !matches!(coverage_result, GlyphCoverageCheckResult::Complete) {
            self.font_fallbacks_for_request(&request, &primary_font, reference_text)
        } else {
            Vec::new()
        };

        let fonts = core::iter::once(primary_font)
            .chain(fallbacks.iter().filter_map(|fallback_request| {
                if matches!(coverage_result, GlyphCoverageCheckResult::Complete) {
                    return None;
                }

                let fallback_font = self.load_single_font(fallback_request);

                coverage_result = self.check_and_update_script_coverage(
                    &mut scripts_required,
                    &mut chars_required,
                    fallback_font.fontdb_face_id,
                );

                if matches!(coverage_result, GlyphCoverageCheckResult::Improved) {
                    Some(fallback_font)
                } else {
                    None
                }
            }))
            .map(|font| ScaledFont { font, pixel_size: request.pixel_size.unwrap() })
            .collect::<SharedVector<_>>();

        Font {
            fonts,
            //text_context: self.text_context.clone(),
            pixel_size: request.pixel_size.unwrap(),
        }
    }

    #[cfg(target_os = "macos")]
    fn font_fallbacks_for_request(
        &self,
        _request: &FontRequest,
        _primary_font: &LoadedFont,
        _reference_text: &str,
    ) -> Vec<FontRequest> {
        let requested_font = match core_text::font::new_from_name(
            &_request.family.as_ref().map_or_else(|| "", |s| s.as_str()),
            _request.pixel_size.unwrap_or_default() as f64,
        ) {
            Ok(f) => f,
            Err(_) => return vec![],
        };

        core_text::font::cascade_list_for_languages(
            &requested_font,
            &core_foundation::array::CFArray::from_CFTypes(&[]),
        )
        .iter()
        .map(|fallback_descriptor| FontRequest {
            family: Some(fallback_descriptor.family_name().into()),
            weight: _request.weight,
            pixel_size: _request.pixel_size,
            letter_spacing: _request.letter_spacing,
        })
        .filter(|request| self.is_known_family(request))
        .collect::<Vec<_>>()
    }

    #[cfg(target_os = "windows")]
    fn font_fallbacks_for_request(
        &self,
        request: &FontRequest,
        _primary_font: &LoadedFont,
        reference_text: &str,
    ) -> Vec<FontRequest> {
        let system_font_fallback = match dwrote::FontFallback::get_system_fallback() {
            Some(fallback) => fallback,
            None => return Vec::new(),
        };
        let font_collection = dwrote::FontCollection::get_system(false);
        let base_family = Some(request.family.as_ref().map_or_else(|| "", |s| s.as_str()));

        let reference_text_utf16: Vec<u16> = reference_text.encode_utf16().collect();

        // Hack to implement the minimum interface for direct write. We have yet to provide the correct
        // locale (but return an empty string for now). This struct stores the number of utf-16 characters
        // so that in get_locale_name it can return that the (empty) locale applies all the characters after
        // `text_position`, by returning the count.
        struct TextAnalysisHack(u32);
        impl dwrote::TextAnalysisSourceMethods for TextAnalysisHack {
            fn get_locale_name<'a>(
                &'a self,
                text_position: u32,
            ) -> (std::borrow::Cow<'a, str>, u32) {
                ("".into(), self.0 - text_position)
            }

            // We should do better on this one, too...
            fn get_paragraph_reading_direction(
                &self,
            ) -> winapi::um::dwrite::DWRITE_READING_DIRECTION {
                winapi::um::dwrite::DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
            }
        }

        let text_analysis_source = dwrote::TextAnalysisSource::from_text_and_number_subst(
            Box::new(TextAnalysisHack(reference_text_utf16.len() as u32)),
            std::borrow::Cow::Borrowed(&reference_text_utf16),
            dwrote::NumberSubstitution::new(
                winapi::um::dwrite::DWRITE_NUMBER_SUBSTITUTION_METHOD_NONE,
                "",
                true,
            ),
        );

        let mut fallback_fonts = Vec::new();

        let mut utf16_pos = 0;

        while utf16_pos < reference_text_utf16.len() {
            let fallback_result = system_font_fallback.map_characters(
                &text_analysis_source,
                utf16_pos as u32,
                (reference_text_utf16.len() - utf16_pos) as u32,
                &font_collection,
                base_family,
                dwrote::FontWeight::Regular,
                dwrote::FontStyle::Normal,
                dwrote::FontStretch::Normal,
            );

            if let Some(fallback_font) = fallback_result.mapped_font {
                let family = fallback_font.family_name();

                let fallback = FontRequest {
                    family: Some(family.into()),
                    weight: request.weight,
                    pixel_size: request.pixel_size,
                    letter_spacing: request.letter_spacing,
                };
                if self.is_known_family(&fallback) {
                    fallback_fonts.push(fallback)
                }
            } else {
                break;
            }

            utf16_pos += fallback_result.mapped_length;
        }

        fallback_fonts
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows"), not(target_arch = "wasm32")))]
    fn font_fallbacks_for_request(
        &self,
        _request: &FontRequest,
        _primary_font: &LoadedFont,
        _reference_text: &str,
    ) -> Vec<FontRequest> {
        self.fontconfig_fallback_families
            .iter()
            .map(|family_name| FontRequest {
                family: Some(family_name.into()),
                weight: _request.weight,
                pixel_size: _request.pixel_size,
                letter_spacing: _request.letter_spacing,
            })
            .filter(|request| self.is_known_family(request))
            .collect()
    }

    #[cfg(target_arch = "wasm32")]
    fn font_fallbacks_for_request(
        &self,
        _request: &FontRequest,
        _primary_font: &LoadedFont,
        _reference_text: &str,
    ) -> Vec<FontRequest> {
        [FontRequest {
            family: Some("DejaVu Sans".into()),
            weight: _request.weight,
            pixel_size: _request.pixel_size,
            letter_spacing: _request.letter_spacing,
        }]
        .iter()
        .filter(|request| self.is_known_family(request))
        .cloned()
        .collect()
    }

    fn is_known_family(&self, request: &FontRequest) -> bool {
        request
            .family
            .as_ref()
            .map(|family_name| self.available_families.contains(family_name))
            .unwrap_or(false)
    }

    // From the set of script without coverage, remove all entries that are known to be covered by
    // the given face_id. Any yet unknown script coverage for the face_id is updated (hence
    // mutable self).
    fn check_and_update_script_coverage(
        &mut self,
        scripts_without_coverage: &mut HashMap<unicode_script::Script, char>,
        chars_without_coverage: &mut HashSet<char>,
        face_id: fontdb::ID,
    ) -> GlyphCoverageCheckResult {
        //eprintln!("required scripts {:#?}", required_scripts);
        let coverage = self.loaded_font_coverage.entry(face_id).or_default();

        let mut scripts_that_need_checking = Vec::new();
        let mut chars_that_need_checking = Vec::new();

        let old_uncovered_scripts_count = scripts_without_coverage.len();
        let old_uncovered_chars_count = chars_without_coverage.len();

        scripts_without_coverage.retain(|script, sample| {
            coverage.supported_scripts.get(script).map_or_else(
                || {
                    scripts_that_need_checking.push((*script, *sample));
                    true // this may or may not be supported, so keep it in scripts_without_coverage
                },
                |has_coverage| !has_coverage,
            )
        });

        chars_without_coverage.retain(|ch| {
            coverage.exact_glyph_coverage.get(ch).map_or_else(
                || {
                    chars_that_need_checking.push(*ch);
                    true // this may or may not be supported, so keep it in chars_without_coverage
                },
                |has_coverage| !has_coverage,
            )
        });

        if !scripts_that_need_checking.is_empty() || !chars_that_need_checking.is_empty() {
            self.available_fonts.with_face_data(face_id, |face_data, face_index| {
                let face = ttf_parser::Face::from_slice(face_data, face_index).unwrap();

                for (unchecked_script, sample_char) in scripts_that_need_checking {
                    let glyph_coverage = face.glyph_index(sample_char).is_some();
                    coverage.supported_scripts.insert(unchecked_script, glyph_coverage);

                    if glyph_coverage {
                        scripts_without_coverage.remove(&unchecked_script);
                    }
                }

                for unchecked_char in chars_that_need_checking {
                    let glyph_coverage = face.glyph_index(unchecked_char).is_some();
                    coverage.exact_glyph_coverage.insert(unchecked_char, glyph_coverage);

                    if glyph_coverage {
                        chars_without_coverage.remove(&unchecked_char);
                    }
                }
            });
        }

        let remaining_required_script_coverage = scripts_without_coverage.len();
        let remaining_required_char_coverage = chars_without_coverage.len();

        if remaining_required_script_coverage < old_uncovered_scripts_count
            || remaining_required_char_coverage < old_uncovered_chars_count
        {
            GlyphCoverageCheckResult::Improved
        } else if scripts_without_coverage.is_empty() && chars_without_coverage.is_empty() {
            GlyphCoverageCheckResult::Complete
        } else {
            GlyphCoverageCheckResult::Incomplete
        }
    }

    fn face_supports_char(&mut self, face_id: fontdb::ID, char: char) -> bool {
        let coverage = self.loaded_font_coverage.entry(face_id).or_default();

        use unicode_script::{Script, UnicodeScript};
        let script = char.script();
        if script == Script::Common || script == Script::Inherited || script == Script::Unknown {
            *coverage.exact_glyph_coverage.entry(char).or_insert_with(|| {
                self.available_fonts
                    .with_face_data(face_id, |face_data, face_index| {
                        let face = ttf_parser::Face::from_slice(face_data, face_index).unwrap();
                        face.glyph_index(char).is_some()
                    })
                    .unwrap_or(false)
            })
        } else {
            *coverage.supported_scripts.entry(script).or_insert_with(|| {
                self.available_fonts
                    .with_face_data(face_id, |face_data, face_index| {
                        let face = ttf_parser::Face::from_slice(face_data, face_index).unwrap();
                        face.glyph_index(char).is_some()
                    })
                    .unwrap_or(false)
            })
        }
    }
}

/// Layout the given string in lines, and call the `layout_line` callback with the line to draw at position y.
/// The signature of the `layout_line` function is: `(canvas, text, pos, start_index, line_metrics)`.
/// start index is the starting byte of the text in the string.
/// Returns the baseline y coordinate.
pub(crate) fn layout_text_lines(
    string: &str,
    font: &Font,
    Size { width: max_width, height: max_height, .. }: Size,
    (horizontal_alignment, vertical_alignment): (TextHorizontalAlignment, TextVerticalAlignment),
    wrap: TextWrap,
    overflow: TextOverflow,
    single_line: bool,
    physical_letter_spacing: Option<f32>,
    paint: femtovg::Paint,
    mut layout_line: impl FnMut(&str, Point, usize, &femtovg::TextMetrics),
) -> f32 {
    let wrap = wrap == TextWrap::word_wrap;
    let elide = overflow == TextOverflow::elide;

    let text_context = FONT_CACHE.with(|cache| cache.borrow().text_context.clone());
    let font_metrics = text_context.measure_font(paint).unwrap();
    let font_height = font_metrics.height();

    let text_height = || {
        if single_line {
            font_height
        } else {
            // Note: this is kind of doing twice the layout because text_size also does it
            font.text_size(
                physical_letter_spacing,
                string,
                if wrap { Some(max_width) } else { None },
            )
            .height
        }
    };

    let mut process_line = |text: &str,
                            y: f32,
                            start: usize,
                            line_metrics: &femtovg::TextMetrics| {
        let x = match horizontal_alignment {
            TextHorizontalAlignment::left => 0.,
            TextHorizontalAlignment::center => {
                max_width / 2. - f32::min(max_width, line_metrics.width()) / 2.
            }
            TextHorizontalAlignment::right => max_width - f32::min(max_width, line_metrics.width()),
        };
        layout_line(text, Point::new(x, y), start, line_metrics);
    };

    let baseline_y = match vertical_alignment {
        TextVerticalAlignment::top => 0.,
        TextVerticalAlignment::center => max_height / 2. - text_height() / 2.,
        TextVerticalAlignment::bottom => max_height - text_height(),
    };
    let mut y = baseline_y;
    let mut start = 0;
    'lines: while start < string.len() && y + font_height <= max_height {
        if wrap && (!elide || y + 2. * font_height <= max_height) {
            let index = text_context.break_text(max_width, &string[start..], paint).unwrap();
            if index == 0 {
                // FIXME the word is too big to be shown, but we should still break, ideally
                break;
            }
            let index = start + index;
            let line = &string[start..index];
            let text_metrics = text_context.measure_text(0., 0., line, paint).unwrap();
            process_line(line, y, start, &text_metrics);
            y += font_height;
            start = index;
        } else {
            let index = if single_line {
                string.len()
            } else {
                string[start..].find('\n').map_or(string.len(), |i| start + i + 1)
            };
            let line = &string[start..index];
            let text_metrics = text_context.measure_text(0., 0., line, paint).unwrap();
            let elide_last_line =
                elide && index < string.len() && y + 2. * font_height > max_height;
            if text_metrics.width() > max_width || elide_last_line {
                let w = max_width
                    - if elide {
                        text_context.measure_text(0., 0., "…", paint).unwrap().width()
                    } else {
                        0.
                    };
                let mut current_x = 0.;
                for glyph in &text_metrics.glyphs {
                    current_x += glyph.advance_x;
                    if current_x >= w {
                        let txt = &line[..glyph.byte_index];
                        if elide {
                            let elided = format!("{}…", txt);
                            process_line(&elided, y, start, &text_metrics);
                        } else {
                            process_line(txt, y, start, &text_metrics);
                        }
                        y += font_height;
                        start = index;
                        continue 'lines;
                    }
                }
                if elide_last_line {
                    let elided = format!("{}…", line);
                    process_line(&elided, y, start, &text_metrics);
                    y += font_height;
                    start = index;
                    continue 'lines;
                }
            }
            process_line(line, y, start, &text_metrics);
            y += font_height;
            start = index;
        }
    }
    baseline_y
}
