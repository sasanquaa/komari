use std::{
    collections::HashMap,
    env,
    fmt::Debug,
    sync::{Arc, LazyLock, Mutex},
};

use anyhow::{Result, anyhow, bail};
use base64::{Engine, prelude::BASE64_STANDARD};
use log::{debug, error, info};
#[cfg(test)]
use mockall::automock;
use opencv::{
    boxed_ref::BoxedRef,
    core::{
        BORDER_CONSTANT, CMP_EQ, CMP_GT, CV_8U, CV_32FC3, CV_32S, Mat, MatExprTraitConst, MatTrait,
        MatTraitConst, MatTraitConstManual, ModifyInplace, Point, Range, Rect, Scalar, Size,
        ToInputArray, Vec3b, Vector, add, add_weighted_def, bitwise_and_def, compare,
        copy_make_border, divide2_def, extract_channel, find_non_zero, min_max_loc, no_array,
        subtract_def, transpose_nd,
    },
    imgcodecs::{self, IMREAD_COLOR, IMREAD_GRAYSCALE, imdecode, imencode_def},
    imgproc::{
        CC_STAT_AREA, CC_STAT_HEIGHT, CC_STAT_LEFT, CC_STAT_TOP, CC_STAT_WIDTH,
        CHAIN_APPROX_SIMPLE, COLOR_BGR2HSV_FULL, COLOR_BGR2RGB, COLOR_BGRA2BGR, COLOR_BGRA2GRAY,
        INTER_CUBIC, INTER_LINEAR, MORPH_RECT, RETR_EXTERNAL, THRESH_BINARY, TM_CCOEFF_NORMED,
        TM_SQDIFF_NORMED, bounding_rect, connected_components_with_stats, contour_area,
        cvt_color_def, dilate_def, find_contours_def, get_structuring_element_def, match_template,
        min_area_rect, min_enclosing_triangle, resize, threshold,
    },
};
use ort::{
    execution_providers::CUDAExecutionProvider,
    session::{Session, SessionInputValue, SessionOutputs},
    value::TensorRef,
};
use strsim::jaro_winkler;

use crate::mat::OwnedMat;
use crate::{bridge::KeyKind, models::Localization};

#[derive(Debug, Copy, Clone)]
pub struct Arrow {
    pub key: KeyKind,
    pub region: Rect,
}

/// Struct for storing information about the spinning arrows.
#[derive(Debug, Copy, Clone)]
pub struct SpinArrow {
    /// The centroid of the spinning arrow relative to the whole image.
    pub centroid: Point,
    /// The region of the spinning arrow relative to the whole image.
    pub region: Rect,
    /// The last arrow head relative to the centroid.
    pub last_arrow_head: Option<Point>,
    /// Final result of spinning arrow.
    pub final_arrow: Option<KeyKind>,

    #[cfg(debug_assertions)]
    pub last_last_arrow_head: Option<Point>,
}

#[derive(Clone, Copy, Debug)]
pub enum OtherPlayerKind {
    Guildie,
    Stranger,
    Friend,
}

#[derive(Debug)]
pub enum FamiliarLevel {
    Level5,
    LevelOther,
}

#[derive(Debug)]
pub enum FamiliarRank {
    Rare,
    Epic,
}

#[derive(Debug)]
pub enum BuffKind {
    Rune,
    Familiar,
    SayramElixir,
    AureliaElixir,
    ExpCouponX2,
    ExpCouponX3,
    ExpCouponX4,
    BonusExpCoupon,
    MvpBonusExpCoupon,
    LegionWealth,
    LegionLuck,
    WealthAcquisitionPotion,
    ExpAccumulationPotion,
    SmallWealthAcquisitionPotion,
    SmallExpAccumulationPotion,
    ForTheGuild,
    HardHitter,
    ExtremeRedPotion,
    ExtremeBluePotion,
    ExtremeGreenPotion,
    ExtremeGoldPotion,
}

#[derive(Debug)]
pub enum QuickSlotsHexaBooster {
    Available,
    Unavailable,
}

#[derive(Debug)]
pub enum SolErda {
    Full,
    AtLeastOne,
    Empty,
}

/// A trait for detecting objects from provided frame.
#[cfg_attr(test, automock)]
pub trait Detector: Debug + Send + Sync {
    /// Gets the original [`OwnedMat`] as a [`BoxedRef`].
    fn mat(&self) -> BoxedRef<'_, Mat>;

    /// Gets the grayscale version.
    fn grayscale(&self) -> &Mat;

    /// Detects a list of mobs.
    ///
    /// Returns a list of mobs coordinate relative to minimap coordinate.
    fn detect_mobs(
        &self,
        minimap: Rect,
        bound: Rect,
        player: Point,
        player_name: Option<String>,
    ) -> Result<Vec<Point>>;

    /// Detects whether to press ESC for unstucking.
    fn detect_esc_settings(&self) -> bool;

    /// Detects the popup `Confirm` button.
    fn detect_popup_confirm_button(&self) -> Result<Rect>;

    /// Detects the new popup `OK` button.
    fn detect_popup_ok_new_button(&self) -> Result<Rect>;

    /// Detects whether there is an elite boss bar.
    fn detect_elite_boss_bar(&self) -> bool;

    /// Detects the minimap.
    ///
    /// The `border_threshold` determines the "whiteness" (grayscale value from 0..255) of
    /// the minimap's white border.
    fn detect_minimap(&self, border_threshold: u8) -> Result<Rect>;

    /// Detects the portals from the given `minimap` rectangle.
    ///
    /// Returns `Rect` relative to `minimap` coordinate.
    fn detect_minimap_portals(&self, minimap: Rect) -> Vec<Rect>;

    /// Detects the rune from the given `minimap` rectangle.
    ///
    /// Returns `Rect` relative to `minimap` coordinate.
    fn detect_minimap_rune(&self, minimap: Rect) -> Result<Rect>;

    /// Detects the player in the provided `minimap` rectangle.
    ///
    /// Returns `Rect` relative to `minimap` coordinate.
    fn detect_player(&self, minimap: Rect) -> Result<Rect>;

    /// Detects the player's name.
    ///
    /// Only English name is currently supported.
    fn detect_player_name(&self) -> Result<String>;

    /// Detects whether a player of `kind` is in the minimap.
    fn detect_player_kind(&self, minimap: Rect, kind: OtherPlayerKind) -> bool;

    /// Detects whether the player is dead.
    fn detect_player_is_dead(&self) -> bool;

    /// Detects whether the player is in cash shop.
    fn detect_player_in_cash_shop(&self) -> bool;

    /// Detects the player health bar.
    ///
    /// This is the biggest red health bar below the name.
    fn detect_player_health_bar(&self) -> Result<Rect>;

    /// Detects the player current and max health bars.
    ///
    /// These are the two smaller bars extracted from `health_bar`.
    fn detect_player_current_max_health_bars(&self, health_bar: Rect) -> Result<(Rect, Rect)>;

    /// Detects the player current health and max health.
    fn detect_player_health(&self, current_bar: Rect, max_bar: Rect) -> Result<(u32, u32)>;

    /// Detects whether the player has a buff specified by `kind`.
    fn detect_player_buff(&self, kind: BuffKind) -> bool;

    /// Detects arrows from the given RGBA `Mat` image.
    fn detect_rune_arrows(&self, ignore: Vec<Rect>) -> Vec<Arrow>;

    fn detect_rune_initial_spin_arrows(&self) -> Vec<SpinArrow>;

    fn detect_rune_spin_arrow(&self, arrow: SpinArrow) -> SpinArrow;

    /// Detects the Erda Shower skill from the given BGRA `Mat` image.
    fn detect_erda_shower(&self) -> Result<Rect>;

    /// Detects familiar menu save button.
    fn detect_familiar_save_button(&self) -> Result<Rect>;

    /// Detects familiar menu level button.
    fn detect_familiar_level_button(&self) -> Result<Rect>;

    /// Detects the familiar slots assuming the familiar menu opened.
    ///
    /// Returns a pair of `(Rect, bool)` with `bool` of `true` indicating the slot is free.
    fn detect_familiar_slots(&self) -> Vec<(Rect, bool)>;

    /// Detects whether the familiar slot is free.
    fn detect_familiar_slot_is_free(&self, slot: Rect) -> bool;

    /// Detects the currently mouse hovering familiar level.
    fn detect_familiar_hover_level(&self) -> Result<FamiliarLevel>;

    /// Detects all the familiar cards assuming the familiar menu opened.
    fn detect_familiar_cards(&self) -> Vec<(Rect, FamiliarRank)>;

    /// Detects familiar menu setup's tab scrollbar assuming familiar menu opened.
    fn detect_familiar_scrollbar(&self) -> Result<Rect>;

    /// Detects whether the familiar menu is opened.
    fn detect_familiar_menu_opened(&self) -> bool;

    /// Detects whether the familiar essence depleted assuming already buffed.
    fn detect_familiar_essence_depleted(&self) -> bool;

    /// Detects whether the change channel menu is opened.
    fn detect_change_channel_menu_opened(&self) -> bool;

    /// Detects whether the admin image is visible inside the currently opened popup/dialog.
    fn detect_admin_visible(&self) -> bool;

    /// Detects whether there is a timer (e.g. from using booster).
    fn detect_timer_visible(&self) -> bool;

    /// Detects the transparent shape's lie detector popup.
    fn detect_lie_detector_shape(&self) -> Result<Rect>;

    /// Detects whether transparent shape's lie detector is preparing.
    fn detect_lie_detector_shape_preparing(&self) -> bool;

    /// Detects the violetta's lie detector popup.
    fn detect_lie_detector_violetta(&self) -> Result<Rect>;

    /// Detects whether violetta's lie detector is preparing.
    fn detect_lie_detector_violetta_preparing(&self) -> bool;

    /// Detects the state for HEXA Booster in the quick slots.
    fn detect_quick_slots_hexa_booster(&self) -> Result<QuickSlotsHexaBooster>;

    /// Detects the HEXA icon in quick menu.
    fn detect_hexa_quick_menu(&self) -> Result<Rect>;

    /// Detects the `Erda conversion` button in HEXA matrix.
    fn detect_hexa_erda_conversion_button(&self) -> Result<Rect>;

    /// Detects the `HEXA Booster` button in `Erda conversion` menu.
    fn detect_hexa_booster_button(&self) -> Result<Rect>;

    /// Detects the `MAX` button in `Erda conversion` menu.
    fn detect_hexa_max_button(&self) -> Result<Rect>;

    /// Detects the `Convert` button in `Erda conversion` menu.
    fn detect_hexa_convert_button(&self) -> Result<Rect>;

    /// Detects the Sol Erda state from the tracker menu.
    fn detect_hexa_sol_erda(&self) -> Result<SolErda>;

    /// Detects a list of transparent shapes during lie detector event.
    ///
    /// The returned [`Rect`]s have coordinates relative to `region`.
    fn detect_transparent_shapes(&self, region: Rect) -> Vec<(Rect, f32)>;

    /// Detects a list of mushrooms during Violetta lie detector event.
    ///
    /// The returned [`Rect`]s have coordinates relative to `region`.
    fn detect_violetta_mushrooms(&self, region: Rect) -> Vec<(Rect, f32)>;

    /// Detects violetta's face.
    fn detect_violetta_face(&self, region: Rect) -> Result<Rect>;

    /// Detects violetta's number boxes.
    fn detect_violetta_numbers(&self, region: Rect) -> Vec<Rect>;
}

type MatFn = Box<dyn FnOnce() -> Mat + Send>;

/// A detector that lazily transform `Mat`.
#[derive(Debug)]
pub struct DefaultDetector {
    bgra: Arc<OwnedMat>,
    bgr: LazyLock<Mat, MatFn>,
    grayscale: LazyLock<Mat, MatFn>,
    localization: Arc<Localization>,
}

impl DefaultDetector {
    /// Creates a default implementation of [`Detector`] from the given BGRA `mat`.
    pub fn new(mat: OwnedMat, localization: Arc<Localization>) -> Self {
        let bgra = Arc::new(mat);

        let cloned = bgra.clone();
        let bgr = LazyLock::<Mat, MatFn>::new(Box::new(move || to_bgr(&cloned.as_mat())));

        let cloned = bgra.clone();
        let grayscale =
            LazyLock::<Mat, MatFn>::new(Box::new(move || to_grayscale(&cloned.as_mat(), true)));

        Self {
            bgra,
            bgr,
            grayscale,
            localization,
        }
    }

    fn bgra(&self) -> BoxedRef<'_, Mat> {
        self.bgra.as_mat()
    }

    fn bgr(&self) -> &Mat {
        &self.bgr
    }
}

impl Detector for DefaultDetector {
    fn mat(&self) -> BoxedRef<'_, Mat> {
        self.bgra()
    }

    fn grayscale(&self) -> &Mat {
        &self.grayscale
    }

    fn detect_mobs(
        &self,
        minimap: Rect,
        bound: Rect,
        player: Point,
        player_name: Option<String>,
    ) -> Result<Vec<Point>> {
        detect_mobs(self.bgr(), minimap, bound, player, player_name)
    }

    fn detect_esc_settings(&self) -> bool {
        detect_esc_settings(self.bgr(), self.grayscale(), &self.localization)
    }

    fn detect_popup_confirm_button(&self) -> Result<Rect> {
        detect_popup_confirm_button(self.grayscale(), &self.localization)
    }

    fn detect_popup_ok_new_button(&self) -> Result<Rect> {
        detect_popup_ok_new_button(self.grayscale(), &self.localization)
    }

    fn detect_elite_boss_bar(&self) -> bool {
        detect_elite_boss_bar(self.grayscale())
    }

    fn detect_minimap(&self, border_threshold: u8) -> Result<Rect> {
        detect_minimap(self.bgr(), border_threshold)
    }

    fn detect_minimap_portals(&self, minimap: Rect) -> Vec<Rect> {
        detect_minimap_portals(self.bgr().roi(minimap).unwrap())
    }

    fn detect_minimap_rune(&self, minimap: Rect) -> Result<Rect> {
        detect_minimap_rune(&self.bgr().roi(minimap).unwrap())
    }

    fn detect_player(&self, minimap: Rect) -> Result<Rect> {
        detect_player(&self.bgr().roi(minimap).unwrap())
    }

    fn detect_player_name(&self) -> Result<String> {
        detect_player_name(self.bgr(), self.grayscale())
    }

    fn detect_player_kind(&self, minimap: Rect, kind: OtherPlayerKind) -> bool {
        detect_player_kind(&self.bgr().roi(minimap).unwrap(), kind)
    }

    fn detect_player_is_dead(&self) -> bool {
        detect_player_is_dead(self.grayscale())
    }

    fn detect_player_in_cash_shop(&self) -> bool {
        detect_player_in_cash_shop(self.grayscale(), &self.localization)
    }

    fn detect_player_health_bar(&self) -> Result<Rect> {
        detect_player_health_bar(self.grayscale())
    }

    fn detect_player_current_max_health_bars(&self, health_bar: Rect) -> Result<(Rect, Rect)> {
        detect_player_current_max_health_bars(self.bgr(), self.grayscale(), health_bar)
    }

    fn detect_player_health(&self, current_bar: Rect, max_bar: Rect) -> Result<(u32, u32)> {
        detect_player_health(self.bgr(), current_bar, max_bar)
    }

    fn detect_player_buff(&self, kind: BuffKind) -> bool {
        let mat = match kind {
            BuffKind::Rune
            | BuffKind::Familiar
            | BuffKind::SayramElixir
            | BuffKind::AureliaElixir
            | BuffKind::ExpCouponX2
            | BuffKind::ExpCouponX3
            | BuffKind::ExpCouponX4
            | BuffKind::BonusExpCoupon
            | BuffKind::MvpBonusExpCoupon
            | BuffKind::ForTheGuild
            | BuffKind::HardHitter => &to_buffs_region(self.grayscale()),
            BuffKind::LegionWealth
            | BuffKind::LegionLuck
            | BuffKind::WealthAcquisitionPotion
            | BuffKind::ExpAccumulationPotion
            | BuffKind::SmallWealthAcquisitionPotion
            | BuffKind::SmallExpAccumulationPotion
            | BuffKind::ExtremeRedPotion
            | BuffKind::ExtremeBluePotion
            | BuffKind::ExtremeGreenPotion
            | BuffKind::ExtremeGoldPotion => &to_buffs_region(self.bgr()),
        };
        detect_player_buff(mat, kind)
    }

    fn detect_rune_arrows(&self, ignore: Vec<Rect>) -> Vec<Arrow> {
        detect_rune_arrows(self.bgr(), ignore).unwrap_or_default()
    }

    fn detect_rune_initial_spin_arrows(&self) -> Vec<SpinArrow> {
        detect_rune_initial_spin_arrows(self.bgr())
    }

    fn detect_rune_spin_arrow(&self, mut arrow: SpinArrow) -> SpinArrow {
        let _ = detect_rune_spin_arrow(self.bgr(), &mut arrow);
        arrow
    }

    fn detect_erda_shower(&self) -> Result<Rect> {
        detect_erda_shower(self.grayscale())
    }

    fn detect_familiar_save_button(&self) -> Result<Rect> {
        detect_familiar_save_button(self.bgr(), &self.localization)
    }

    fn detect_familiar_level_button(&self) -> Result<Rect> {
        detect_familiar_level_button(self.bgr(), &self.localization)
    }

    fn detect_familiar_slots(&self) -> Vec<(Rect, bool)> {
        detect_familiar_slots(self.bgr())
    }

    fn detect_familiar_slot_is_free(&self, slot: Rect) -> bool {
        detect_familiar_slot_is_free(&self.bgr().roi(slot).unwrap())
    }

    fn detect_familiar_hover_level(&self) -> Result<FamiliarLevel> {
        detect_familiar_hover_level(self.bgr())
    }

    fn detect_familiar_cards(&self) -> Vec<(Rect, FamiliarRank)> {
        detect_familiar_cards(self.bgr())
    }

    fn detect_familiar_scrollbar(&self) -> Result<Rect> {
        detect_familiar_scrollbar(&to_grayscale(&self.bgra(), false))
    }

    fn detect_familiar_menu_opened(&self) -> bool {
        detect_familiar_menu_opened(self.grayscale())
    }

    fn detect_familiar_essence_depleted(&self) -> bool {
        detect_familiar_essence_depleted(&to_buffs_region(self.grayscale()))
    }

    fn detect_change_channel_menu_opened(&self) -> bool {
        detect_change_channel_menu_opened(self.grayscale(), &self.localization)
    }

    fn detect_admin_visible(&self) -> bool {
        detect_admin_visible(self.grayscale())
    }

    fn detect_timer_visible(&self) -> bool {
        detect_timer_visible(self.grayscale(), &self.localization)
    }

    fn detect_lie_detector_shape(&self) -> Result<Rect> {
        detect_lie_detector_shape(self.bgr())
    }

    fn detect_lie_detector_shape_preparing(&self) -> bool {
        detect_lie_detector_shape_preparing(self.bgr()).is_ok()
    }

    fn detect_lie_detector_violetta(&self) -> Result<Rect> {
        detect_lie_detector_violetta(self.bgr())
    }

    fn detect_lie_detector_violetta_preparing(&self) -> bool {
        detect_lie_detector_violetta_preparing(self.bgr()).is_ok()
    }

    fn detect_quick_slots_hexa_booster(&self) -> Result<QuickSlotsHexaBooster> {
        detect_quick_slots_hexa_booster(&to_quick_slots_region(self.grayscale()).0)
    }

    fn detect_hexa_quick_menu(&self) -> Result<Rect> {
        detect_hexa_quick_menu(self.grayscale())
    }

    fn detect_hexa_erda_conversion_button(&self) -> Result<Rect> {
        detect_hexa_erda_conversion_button(self.bgr(), &self.localization)
    }

    fn detect_hexa_booster_button(&self) -> Result<Rect> {
        detect_hexa_booster_button(self.bgr(), &self.localization)
    }

    fn detect_hexa_max_button(&self) -> Result<Rect> {
        detect_hexa_max_button(self.bgr(), &self.localization)
    }

    fn detect_hexa_convert_button(&self) -> Result<Rect> {
        detect_hexa_convert_button(self.bgr(), &self.localization)
    }

    fn detect_hexa_sol_erda(&self) -> Result<SolErda> {
        detect_hexa_sol_erda(self.grayscale())
    }

    fn detect_transparent_shapes(&self, region: Rect) -> Vec<(Rect, f32)> {
        detect_transparent_shapes(&self.bgr().roi(region).unwrap())
    }

    fn detect_violetta_mushrooms(&self, region: Rect) -> Vec<(Rect, f32)> {
        detect_violetta_mushrooms(&self.bgr().roi(region).unwrap())
    }

    fn detect_violetta_face(&self, region: Rect) -> Result<Rect> {
        detect_violetta_face(&self.bgr().roi(region).unwrap())
    }

    fn detect_violetta_numbers(&self, region: Rect) -> Vec<Rect> {
        detect_violetta_numbers(&self.bgr().roi(region).unwrap())
    }
}

fn detect_mobs(
    bgr: &impl MatTraitConst,
    minimap: Rect,
    bound: Rect,
    player: Point,
    player_name: Option<String>,
) -> Result<Vec<Point>> {
    static MOB_MODEL: LazyLock<Mutex<Session>> = LazyLock::new(|| {
        Mutex::new(
            build_session(include_bytes!(env!("MOB_MODEL")))
                .expect("build mob detection session successfully"),
        )
    });

    /// Approximates the mob coordinate on screen to mob coordinate on minimap.
    ///
    /// This function tries to approximate the delta `(dx, dy)` that the player needs to move
    /// in relative to the minimap coordinate in order to reach the mob. And returns the mob
    /// coordinate on the minimap by adding the delta to the player position.
    #[inline]
    fn to_minimap_coordinate(
        mob_bbox: Rect,
        minimap_bbox: Rect,
        mobbing_bound: Rect,
        player: Point,
        player_on_screen: Option<Rect>,
        mat_size: Size,
    ) -> Option<Point> {
        const X_SCALE: f32 = 0.059_375;
        const Y_SCALE: f32 = 0.036_111;

        let x_mob_mid = mob_bbox.x + mob_bbox.width / 2;
        let x_screen_delta = if let Some(screen) = player_on_screen {
            screen.x + screen.width / 2 - x_mob_mid
        } else {
            mat_size.width / 2 - x_mob_mid
        };
        let x_minimap_delta = (x_screen_delta as f32 * X_SCALE) as i32;

        let y_screen_delta = match player_on_screen {
            Some(screen) => {
                let y_screen_mid = screen.y + screen.height / 2;
                let y_mob = if mob_bbox.y > y_screen_mid {
                    mob_bbox.y + mob_bbox.height
                } else {
                    mob_bbox.y + mob_bbox.height / 2
                };

                y_screen_mid - y_mob
            }
            None => {
                let y_screen_mid = mat_size.height / 2;
                let y_mob = if mob_bbox.y + mob_bbox.height < y_screen_mid {
                    mob_bbox.y
                } else if mob_bbox.y > y_screen_mid {
                    mob_bbox.y + mob_bbox.height
                } else {
                    mob_bbox.y + mob_bbox.height / 2
                };

                y_screen_mid - y_mob
            }
        };
        let y_minimap_delta = (y_screen_delta as f32 * Y_SCALE) as i32;

        let point_x = if x_minimap_delta > 0 {
            (player.x - x_minimap_delta).max(0)
        } else {
            (player.x - x_minimap_delta).min(minimap_bbox.width)
        };
        let point_y = (player.y + y_minimap_delta).max(0).min(minimap_bbox.height);
        // Minus the y by minimap height to make it relative to the minimap top edge
        let point = Point::new(point_x, minimap_bbox.height - point_y);
        if point.x < mobbing_bound.x
            || point.x > mobbing_bound.x + mobbing_bound.width
            || point.y < mobbing_bound.y
            || point.y > mobbing_bound.y + mobbing_bound.height
        {
            None
        } else {
            Some(point)
        }
    }

    let size = bgr.size().unwrap();
    let (mat_in, w_ratio, h_ratio, left, top) = preprocess_for_yolo(bgr);
    let mut model = MOB_MODEL.lock().unwrap();
    let result = model.run([to_input_value(&mat_in)]).unwrap();
    let result = from_output_value(&result);
    // SAFETY: 0..result.rows() is within Mat bounds
    let player_on_screen = player_name.and_then(|name| detect_player_on_screen(bgr, name).ok());
    let points = (0..result.rows())
        .map(|i| unsafe { result.at_row_unchecked::<f32>(i).unwrap() })
        .filter(|pred| pred[4] >= 0.5)
        .map(|pred| remap_from_yolo(pred, size, w_ratio, h_ratio, left, top))
        .filter_map(|bbox| {
            to_minimap_coordinate(bbox, minimap, bound, player, player_on_screen, size)
        })
        .collect::<Vec<_>>();
    Ok(points)
}

fn detect_player_name(bgr: &impl MatTraitConst, grayscale: &impl ToInputArray) -> Result<String> {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("LEVEL_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    let name_area = detect_template(grayscale, &*TEMPLATE, Point::new(55, -6), 0.75)
        .map(|level| Rect::new(level.x, level.y, 100, 25))?;
    let bgr = bgr.roi(name_area)?;

    let (mat_in, w_ratio, h_ratio) = preprocess_for_text_bboxes(&bgr);
    let bboxes = extract_text_bboxes(&mat_in, w_ratio, h_ratio, 0, 0);
    let name_area = name_area - name_area.tl();
    let name_bbox = bboxes
        .iter()
        .find(|&&bbox| (bbox & name_area).area() > 0)
        .copied()
        .ok_or(anyhow!("failed to find name bbox"))?;

    extract_texts(&bgr, &[name_bbox])
        .into_iter()
        .next()
        .ok_or(anyhow!("failed to detect name text"))
}

fn detect_player_on_screen(bgr: &impl MatTraitConst, name: String) -> Result<Rect> {
    let size = bgr.size().unwrap();
    let player_area = Rect::new(
        (size.width as f32 * 0.3) as i32,
        (size.height as f32 * 0.3) as i32,
        (size.width as f32 * 0.45) as i32,
        (size.height as f32 * 0.65) as i32,
    );
    let bgr = bgr.roi(player_area)?;

    let (mat_in, w_ratio, h_ratio) = preprocess_for_text_bboxes_magnified(&bgr, 1.0);
    let bboxes = extract_text_bboxes(&mat_in, w_ratio, h_ratio, 0, 0);
    let texts = extract_texts(&bgr, &bboxes);
    let index = texts
        .iter()
        .enumerate()
        .filter_map(|(index, text)| {
            let score = jaro_winkler(text.as_str(), name.as_str());
            if score < 0.7 {
                return None;
            }
            Some((index, score))
        })
        .max_by(|(_, first), (_, second)| first.partial_cmp(second).unwrap())
        .ok_or(anyhow!("failed to find player bbox"))?
        .0;

    Ok(bboxes[index] + player_area.tl())
}

pub static POPUP_CONFIRM_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("POPUP_CONFIRM_TEMPLATE")),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

pub static POPUP_YES_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(include_bytes!(env!("POPUP_YES_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
});

pub static POPUP_NEXT_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("POPUP_NEXT_TEMPLATE")),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

pub static POPUP_END_CHAT_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("POPUP_END_CHAT_TEMPLATE")),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

pub static POPUP_OK_NEW_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("POPUP_OK_NEW_TEMPLATE")),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

pub static POPUP_OK_OLD_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("POPUP_OK_OLD_TEMPLATE")),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

pub static POPUP_CANCEL_NEW_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("POPUP_CANCEL_NEW_TEMPLATE")),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

pub static POPUP_CANCEL_OLD_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("POPUP_CANCEL_OLD_TEMPLATE")),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

fn detect_esc_settings(
    bgr: &impl ToInputArray,
    grayscale: &impl ToInputArray,
    localization: &Localization,
) -> bool {
    static ESC_MENU_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("ESC_MENU_TEMPLATE")), IMREAD_COLOR).unwrap()
    });

    if detect_template(bgr, &*ESC_MENU_TEMPLATE, Point::default(), 0.75).is_ok() {
        return true;
    }
    if detect_popup_confirm_button(grayscale, localization).is_ok() {
        return true;
    }
    if detect_popup_yes_button(grayscale, localization).is_ok() {
        return true;
    }
    if detect_popup_next_button(grayscale, localization).is_ok() {
        return true;
    }
    if detect_popup_end_chat_button(grayscale, localization).is_ok() {
        return true;
    }
    if detect_popup_ok_new_button(grayscale, localization).is_ok() {
        return true;
    }
    if detect_popup_ok_old_button(grayscale, localization).is_ok() {
        return true;
    }
    if detect_popup_cancel_new_button(grayscale, localization).is_ok() {
        return true;
    }
    if detect_popup_cancel_old_button(grayscale, localization).is_ok() {
        return true;
    }
    if detect_hexa_menu(grayscale) {
        return true;
    }

    false
}

fn detect_popup_confirm_button(
    grayscale: &impl ToInputArray,
    localization: &Localization,
) -> Result<Rect> {
    let template = localization
        .popup_confirm_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, true).ok());

    detect_template(
        grayscale,
        template.as_ref().unwrap_or(&*POPUP_CONFIRM_TEMPLATE),
        Point::default(),
        0.75,
    )
}

fn detect_popup_yes_button(
    grayscale: &impl ToInputArray,
    localization: &Localization,
) -> Result<Rect> {
    let template = localization
        .popup_yes_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, true).ok());

    detect_template(
        grayscale,
        template.as_ref().unwrap_or(&*POPUP_YES_TEMPLATE),
        Point::default(),
        0.75,
    )
}

fn detect_popup_next_button(
    grayscale: &impl ToInputArray,
    localization: &Localization,
) -> Result<Rect> {
    let template = localization
        .popup_next_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, true).ok());

    detect_template(
        grayscale,
        template.as_ref().unwrap_or(&*POPUP_NEXT_TEMPLATE),
        Point::default(),
        0.75,
    )
}

fn detect_popup_end_chat_button(
    grayscale: &impl ToInputArray,
    localization: &Localization,
) -> Result<Rect> {
    let template = localization
        .popup_end_chat_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, true).ok());

    detect_template(
        grayscale,
        template.as_ref().unwrap_or(&*POPUP_END_CHAT_TEMPLATE),
        Point::default(),
        0.75,
    )
}

fn detect_popup_ok_new_button(
    grayscale: &impl ToInputArray,
    localization: &Localization,
) -> Result<Rect> {
    let template = localization
        .popup_ok_new_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, true).ok());

    detect_template(
        grayscale,
        template.as_ref().unwrap_or(&*POPUP_OK_NEW_TEMPLATE),
        Point::default(),
        0.75,
    )
}

fn detect_popup_ok_old_button(
    grayscale: &impl ToInputArray,
    localization: &Localization,
) -> Result<Rect> {
    let template = localization
        .popup_ok_old_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, true).ok());

    detect_template(
        grayscale,
        template.as_ref().unwrap_or(&*POPUP_OK_OLD_TEMPLATE),
        Point::default(),
        0.75,
    )
}

fn detect_popup_cancel_new_button(
    grayscale: &impl ToInputArray,
    localization: &Localization,
) -> Result<Rect> {
    let template = localization
        .popup_cancel_new_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, true).ok());

    detect_template(
        grayscale,
        template.as_ref().unwrap_or(&*POPUP_CANCEL_NEW_TEMPLATE),
        Point::default(),
        0.75,
    )
}

fn detect_popup_cancel_old_button(
    grayscale: &impl ToInputArray,
    localization: &Localization,
) -> Result<Rect> {
    let template = localization
        .popup_cancel_old_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, true).ok());

    detect_template(
        grayscale,
        template.as_ref().unwrap_or(&*POPUP_CANCEL_OLD_TEMPLATE),
        Point::default(),
        0.75,
    )
}

fn detect_elite_boss_bar(grayscale: &impl MatTraitConst) -> bool {
    /// TODO: Support default ratio
    static TEMPLATE_1: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("ELITE_BOSS_BAR_1_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static TEMPLATE_2: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("ELITE_BOSS_BAR_2_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    let size = grayscale.size().unwrap();
    // crop to top part of the image for boss bar
    let crop_y = size.height / 5;
    let crop_bbox = Rect::new(0, 0, size.width, crop_y);
    let boss_bar = grayscale.roi(crop_bbox).unwrap();
    let template_1 = &*TEMPLATE_1;
    let template_2 = &*TEMPLATE_2;
    detect_template(&boss_bar, template_1, Point::default(), 0.9).is_ok()
        || detect_template(&boss_bar, template_2, Point::default(), 0.9).is_ok()
}

fn detect_minimap(bgr: &impl MatTraitConst, border_threshold: u8) -> Result<Rect> {
    static MINIMAP_MODEL: LazyLock<Mutex<Session>> = LazyLock::new(|| {
        Mutex::new(
            build_session(include_bytes!(env!("MINIMAP_MODEL")))
                .expect("build minimap detection session successfully"),
        )
    });

    #[derive(Debug)]
    enum Border {
        Top,
        Bottom,
        Left,
        Right,
    }

    fn scan_border(minimap: &impl MatTraitConst, border: Border, border_threshold: u8) -> i32 {
        let mut counts = HashMap::<u32, u32>::new();
        let is_pixel_above_threshold =
            |pixel: &[u8; 3]| pixel.iter().all(|&v| v >= border_threshold);
        let (primary_len, secondary_len, flip_primary) = match border {
            Border::Top => (minimap.rows(), minimap.cols(), false),
            Border::Bottom => (minimap.rows(), minimap.cols(), true),
            Border::Left => (minimap.cols(), minimap.rows(), false),
            Border::Right => (minimap.cols(), minimap.rows(), true),
        };

        let secondary_start = ((secondary_len - 1) as f32 * 0.1) as i32;
        let secondary_end = secondary_len - secondary_start;

        for secondary in secondary_start..secondary_end {
            let mut count = 0;

            for primary in 0..primary_len {
                let flipped_primary = if flip_primary {
                    primary_len - primary - 1
                } else {
                    primary
                };
                let (row, col) = match border {
                    Border::Top | Border::Bottom => (flipped_primary, secondary),
                    Border::Left | Border::Right => (secondary, flipped_primary),
                };

                let pixel = minimap.at_2d::<Vec3b>(row, col).unwrap();
                if is_pixel_above_threshold(pixel) {
                    count += 1;
                } else {
                    break;
                }
            }

            *counts.entry(count).or_insert(0) += 1;
        }

        counts
            .into_iter()
            .max_by_key(|e| e.1)
            .map(|e| e.0)
            .unwrap_or_default() as i32
    }

    let size = bgr.size().unwrap();
    let (mat_in, w_ratio, h_ratio, left, top) = preprocess_for_yolo(bgr);
    let mut model = MINIMAP_MODEL.lock().unwrap();
    let result = model.run([to_input_value(&mat_in)]).unwrap();
    let mat_out = from_output_value(&result);
    let pred = (0..mat_out.rows())
        // SAFETY: 0..result.rows() is within Mat bounds
        .map(|i| unsafe { mat_out.at_row_unchecked::<f32>(i).unwrap() })
        .max_by(|&a, &b| {
            // a and b have shapes [bbox(4) + class(1)]
            a[4].total_cmp(&b[4])
        })
        .ok_or(anyhow!("minimap detection failed"))?;

    debug!(target: "backend/minimap", "yolo detection: {pred:?}");

    // Extract the thresholded minimap
    let minimap_bbox = remap_from_yolo(pred, size, w_ratio, h_ratio, left, top);
    if minimap_bbox.empty() {
        bail!("minimap is empty");
    }

    let mut minimap_thresh = to_grayscale(&bgr.roi(minimap_bbox).unwrap(), true);
    unsafe {
        // SAFETY: threshold can be called in place.
        minimap_thresh.modify_inplace(|mat, mat_mut| {
            threshold(mat, mat_mut, border_threshold as f64, 255.0, THRESH_BINARY).unwrap()
        });
    }

    // Find the contours with largest area
    let mut contours = Vector::<Vector<Point>>::new();
    find_contours_def(
        &minimap_thresh,
        &mut contours,
        RETR_EXTERNAL,
        CHAIN_APPROX_SIMPLE,
    )
    .unwrap();
    let contour_bbox = contours
        .into_iter()
        .map(|contour| bounding_rect(&contour).unwrap())
        .max_by_key(|bbox| bbox.area())
        .ok_or(anyhow!("minimap contours is empty"))?
        + minimap_bbox.tl();
    if iou(contour_bbox, minimap_bbox) < 0.8 {
        bail!("wrong minimap likely caused by detection during map switching")
    }

    // Scan the 4 borders and crop
    let minimap = bgr.roi(contour_bbox).unwrap();
    let top = scan_border(&minimap, Border::Top, border_threshold);
    let bottom = scan_border(&minimap, Border::Bottom, border_threshold);
    // Left side gets a discount because it is darker than the other three borders
    let left = scan_border(&minimap, Border::Left, border_threshold.saturating_sub(10));
    let right = scan_border(&minimap, Border::Right, border_threshold);

    debug!(target: "backend/minimap", "crop white border left {left}, top {top}, bottom {bottom}, right {right}");

    let bbox = Rect::new(
        left,
        top,
        minimap.cols() - right - left,
        minimap.rows() - bottom - top,
    );
    debug!(target: "backend/minimap", "bbox {bbox:?}");

    Ok(bbox + contour_bbox.tl())
}

fn detect_minimap_portals<T: MatTraitConst + ToInputArray>(minimap_bgr: T) -> Vec<Rect> {
    /// TODO: Support default ratio
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("PORTAL_TEMPLATE")), IMREAD_COLOR).unwrap()
    });
    const PORTAL_EXPAND_SIZE: i32 = 5;

    detect_template_multiple(
        &minimap_bgr,
        &*TEMPLATE,
        no_array(),
        Point::default(),
        16,
        0.7,
    )
    .into_iter()
    .filter_map(|result| result.ok())
    .map(|(bbox, _)| {
        expand_bbox(
            Some(minimap_bgr.size().expect("size available")),
            bbox,
            PORTAL_EXPAND_SIZE,
        )
    })
    .collect::<Vec<_>>()
}

fn detect_minimap_rune(minimap_bgr: &impl ToInputArray) -> Result<Rect> {
    /// TODO: Support default ratio
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("RUNE_TEMPLATE")), IMREAD_COLOR).unwrap()
    });
    static TEMPLATE_MASK: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("RUNE_MASK_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    // Expands by 2 pixels to preserve previous position calculation. Previous template is 11x11
    // while the current template is 9x9.
    detect_template_single(
        minimap_bgr,
        &*TEMPLATE,
        &*TEMPLATE_MASK,
        Point::default(),
        0.75,
    )
    .map(|(bbox, _)| expand_bbox(None, bbox, 1))
}

fn detect_player(minimap_bgr: &impl ToInputArray) -> Result<Rect> {
    /// Stores offsets information for various player templates.
    #[derive(Debug)]
    struct TemplateOffsets {
        template: &'static LazyLock<Mat>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    }

    /// TODO: Support default ratio
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("PLAYER_TEMPLATE")), IMREAD_COLOR).unwrap()
    });
    static TEMPLATE_LEFT_HALF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("PLAYER_LEFT_HALF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static TEMPLATE_RIGHT_HALF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("PLAYER_RIGHT_HALF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static TEMPLATE_TOP_HALF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("PLAYER_TOP_HALF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static TEMPLATE_BOTTOM_HALF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("PLAYER_BOTTOM_HALF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static TEMPLATE_OFFSETS: [TemplateOffsets; 5] = [
        TemplateOffsets {
            template: &TEMPLATE,
            x: -1,
            y: -1,
            width: 2,
            height: 2,
        },
        TemplateOffsets {
            template: &TEMPLATE_LEFT_HALF,
            x: -1,
            y: -1,
            width: 6,
            height: 2,
        },
        TemplateOffsets {
            template: &TEMPLATE_RIGHT_HALF,
            x: -5,
            y: -1,
            width: 6,
            height: 2,
        },
        TemplateOffsets {
            template: &TEMPLATE_TOP_HALF,
            x: -1,
            y: -1,
            width: 2,
            height: 6,
        },
        TemplateOffsets {
            template: &TEMPLATE_BOTTOM_HALF,
            x: -1,
            y: -5,
            width: 2,
            height: 6,
        },
    ];

    // Detect and offset as needed to get a 10x10 for preserving previous behavior.
    for offsets in &TEMPLATE_OFFSETS {
        if let Ok(rect) = detect_template(minimap_bgr, &**offsets.template, Point::default(), 0.75)
        {
            let x = rect.x + offsets.x;
            let y = rect.y + offsets.y;
            let width = rect.width + offsets.width;
            let height = rect.height + offsets.height;

            return Ok(Rect::new(x, y, width, height));
        }
    }

    Err(anyhow!("player not found"))
}

fn detect_player_kind(minimap_bgr: &impl ToInputArray, kind: OtherPlayerKind) -> bool {
    /// TODO: Support default ratio
    static STRANGER_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("PLAYER_STRANGER_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static GUILDIE_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("PLAYER_GUILDIE_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static FRIEND_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("PLAYER_FRIEND_TEMPLATE")), IMREAD_COLOR).unwrap()
    });

    match kind {
        OtherPlayerKind::Stranger => {
            detect_template(minimap_bgr, &*STRANGER_TEMPLATE, Point::default(), 0.85).is_ok()
        }
        OtherPlayerKind::Guildie => {
            detect_template(minimap_bgr, &*GUILDIE_TEMPLATE, Point::default(), 0.85).is_ok()
        }
        OtherPlayerKind::Friend => {
            detect_template(minimap_bgr, &*FRIEND_TEMPLATE, Point::default(), 0.85).is_ok()
        }
    }
}

fn detect_player_is_dead(grayscale: &impl ToInputArray) -> bool {
    /// TODO: Support default ratio
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("TOMB_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    detect_template(grayscale, &*TEMPLATE, Point::default(), 0.8).is_ok()
}

// TODO: Support default ratio
pub static CASH_SHOP_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(include_bytes!(env!("CASH_SHOP_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
});

fn detect_player_in_cash_shop(grayscale: &impl ToInputArray, localization: &Localization) -> bool {
    let template = localization
        .cash_shop_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, true).ok());

    detect_template(
        grayscale,
        template.as_ref().unwrap_or(&*CASH_SHOP_TEMPLATE),
        Point::default(),
        0.7,
    )
    .is_ok()
}

fn detect_player_health_bar<T: MatTraitConst + ToInputArray>(grayscale: &T) -> Result<Rect> {
    /// TODO: Support default ratio
    static HP_BAR_ANCHOR: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HP_BAR_ANCHOR_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    const HP_BAR_X_OFFSET_FROM_ANCHOR_CENTER: i32 = 122;
    const HP_BAR_Y_OFFSET_FROM_ANCHOR_CENTER: i32 = 19;
    const HP_BAR_HALF_WIDTH: i32 = 100;
    const HP_BAR_HALF_HEIGHT: i32 = 10;

    let anchor = detect_template(grayscale, &*HP_BAR_ANCHOR, Point::default(), 0.75)?;
    let size = grayscale.size().expect("has size");
    let hp_bar_x_center = anchor.x + anchor.width / 2 + HP_BAR_X_OFFSET_FROM_ANCHOR_CENTER;
    let hp_bar_y_center = anchor.y + anchor.height / 2 - HP_BAR_Y_OFFSET_FROM_ANCHOR_CENTER;
    if hp_bar_x_center > size.width || hp_bar_y_center < 0 {
        bail!("failed to determine HP bar center");
    }

    let hp_bar_tl = Point::new(
        hp_bar_x_center - HP_BAR_HALF_WIDTH,
        hp_bar_y_center - HP_BAR_HALF_HEIGHT,
    );
    let hp_bar_br = Point::new(
        hp_bar_x_center + HP_BAR_HALF_WIDTH,
        hp_bar_y_center + HP_BAR_HALF_HEIGHT,
    );
    if hp_bar_tl.x < 0 || hp_bar_tl.y < 0 || hp_bar_br.x > size.width || hp_bar_br.y > size.height {
        bail!("failed to determine HP bar");
    }

    Ok(Rect::from_points(hp_bar_tl, hp_bar_br))
}

fn detect_player_current_max_health_bars(
    bgr: &impl MatTraitConst,
    grayscale: &impl MatTraitConst,
    hp_bar: Rect,
) -> Result<(Rect, Rect)> {
    /// TODO: Support default ratio
    static HP_SEPARATOR: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HP_SEPARATOR_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static HP_SHIELD: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("HP_SHIELD_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    let hp_separator = detect_template(
        &grayscale.roi(hp_bar).unwrap(),
        &*HP_SEPARATOR,
        hp_bar.tl(),
        0.7,
    )?;

    let hp_shield = detect_template(
        &grayscale.roi(hp_bar).unwrap(),
        &*HP_SHIELD,
        hp_bar.tl(),
        0.8,
    )
    .ok();

    let left = bgr
        .roi(Rect::new(
            hp_bar.x,
            hp_bar.y,
            hp_separator.x - hp_bar.x,
            hp_bar.height,
        ))
        .unwrap();
    let (left_in, left_w_ratio, left_h_ratio) = preprocess_for_text_bboxes(&left);
    let left_bbox = extract_text_bboxes(&left_in, left_w_ratio, left_h_ratio, hp_bar.x, hp_bar.y)
        .into_iter()
        .min_by_key(|bbox| ((bbox.x + bbox.width) - hp_separator.x).abs())
        .ok_or(anyhow!("failed to detect current health bar"))?;
    let left_bbox_x = hp_shield.map_or(left_bbox.x, |bbox| bbox.x + bbox.width); // When there is shield, skips past it
    let left_bbox = Rect::new(
        left_bbox_x,
        left_bbox.y,
        hp_separator.x - left_bbox_x,
        left_bbox.height,
    );

    let right = bgr
        .roi(Rect::new(
            hp_separator.x + hp_separator.width,
            hp_bar.y,
            (hp_bar.x + hp_bar.width) - (hp_separator.x + hp_separator.width),
            hp_bar.height,
        ))
        .unwrap();
    let (right_in, right_w_ratio, right_h_ratio) = preprocess_for_text_bboxes(&right);
    let right_bbox = extract_text_bboxes(
        &right_in,
        right_w_ratio,
        right_h_ratio,
        hp_separator.x + hp_separator.width,
        hp_bar.y,
    )
    .into_iter()
    .reduce(|acc, cur| acc | cur)
    .ok_or(anyhow!("failed to detect max health bar"))?;

    Ok((left_bbox, right_bbox))
}

fn detect_player_health(
    bgr: &impl MatTraitConst,
    current_bar: Rect,
    max_bar: Rect,
) -> Result<(u32, u32)> {
    let current_health = extract_texts(bgr, &[current_bar]);
    let current_health = current_health
        .first()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(anyhow!("cannot detect current health"))?;
    let max_health = extract_texts(bgr, &[max_bar]);
    let max_health = max_health
        .first()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(anyhow!("cannot detect max health"))?;
    Ok((current_health.min(max_health), max_health))
}

fn detect_player_buff<T: MatTraitConst + ToInputArray>(mat: &T, kind: BuffKind) -> bool {
    /// TODO: Support default ratio
    static RUNE_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("RUNE_BUFF_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });
    static FAMILIAR_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("FAMILIAR_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static SAYRAM_ELIXIR_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("SAYRAM_ELIXIR_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static AURELIA_ELIXIR_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("AURELIA_ELIXIR_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static EXP_COUPON_X2_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXP_COUPON_X2_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static EXP_COUPON_X3_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXP_COUPON_X3_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static EXP_COUPON_X4_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXP_COUPON_X4_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static BONUS_EXP_COUPON_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("BONUS_EXP_COUPON_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static MVP_BONUS_EXP_COUPON_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("MVP_BONUS_EXP_COUPON_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static LEGION_WEALTH_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LEGION_WEALTH_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static LEGION_WEALTH_BUFF_2: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LEGION_WEALTH_BUFF_2_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static LEGION_LUCK_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LEGION_LUCK_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static LEGION_LUCK_BUFF_MASK: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LEGION_LUCK_BUFF_MASK_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static WEALTH_EXP_POTION_MASK: LazyLock<Mat> = LazyLock::new(|| {
        let mut mat = imgcodecs::imdecode(
            include_bytes!(env!("WEALTH_EXP_POTION_MASK_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap();
        unsafe {
            mat.modify_inplace(|mat, mat_mut| {
                mat.convert_to(mat_mut, CV_32FC3, 1.0 / 255.0, 0.0).unwrap();
            });
        }
        mat
    });
    static WEALTH_ACQUISITION_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("WEALTH_ACQUISITION_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static EXP_ACCUMULATION_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXP_ACCUMULATION_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static SMALL_WEALTH_EXP_POTION_MASK: LazyLock<Mat> = LazyLock::new(|| {
        let mut mat = imgcodecs::imdecode(
            include_bytes!(env!("SMALL_WEALTH_EXP_POTION_MASK_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap();
        unsafe {
            mat.modify_inplace(|mat, mat_mut| {
                mat.convert_to(mat_mut, CV_32FC3, 1.0 / 255.0, 0.0).unwrap();
            });
        }
        mat
    });
    static SMALL_WEALTH_ACQUISITION_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("SMALL_WEALTH_ACQUISITION_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static SMALL_EXP_ACCUMULATION_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("SMALL_EXP_ACCUMULATION_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static FOR_THE_GUILD_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("FOR_THE_GUILD_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static HARD_HITTER_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HARD_HITTER_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static EXTREME_RED_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXTREME_RED_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static EXTREME_BLUE_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXTREME_BLUE_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static EXTREME_GREEN_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXTREME_GREEN_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static EXTREME_GOLD_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXTREME_GOLD_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });

    let threshold = match kind {
        BuffKind::AureliaElixir => 0.8,
        BuffKind::LegionWealth | BuffKind::LegionLuck => 0.73,
        BuffKind::SmallWealthAcquisitionPotion
        | BuffKind::SmallExpAccumulationPotion
        | BuffKind::WealthAcquisitionPotion
        | BuffKind::ExpAccumulationPotion => 0.65,
        BuffKind::Rune
        | BuffKind::Familiar
        | BuffKind::SayramElixir
        | BuffKind::ExpCouponX2
        | BuffKind::ExpCouponX3
        | BuffKind::ExpCouponX4
        | BuffKind::BonusExpCoupon
        | BuffKind::MvpBonusExpCoupon
        | BuffKind::ForTheGuild
        | BuffKind::HardHitter
        | BuffKind::ExtremeRedPotion
        | BuffKind::ExtremeBluePotion
        | BuffKind::ExtremeGreenPotion
        | BuffKind::ExtremeGoldPotion => 0.75,
    };
    let template = match kind {
        BuffKind::Rune => &*RUNE_BUFF,
        BuffKind::Familiar => &*FAMILIAR_BUFF,
        BuffKind::SayramElixir => &*SAYRAM_ELIXIR_BUFF,
        BuffKind::AureliaElixir => &*AURELIA_ELIXIR_BUFF,
        BuffKind::ExpCouponX2 => &*EXP_COUPON_X2_BUFF,
        BuffKind::ExpCouponX3 => &*EXP_COUPON_X3_BUFF,
        BuffKind::ExpCouponX4 => &*EXP_COUPON_X4_BUFF,
        BuffKind::BonusExpCoupon => &*BONUS_EXP_COUPON_BUFF,
        BuffKind::MvpBonusExpCoupon => &*MVP_BONUS_EXP_COUPON_BUFF,
        BuffKind::LegionWealth => &*LEGION_WEALTH_BUFF,
        BuffKind::LegionLuck => &*LEGION_LUCK_BUFF,
        BuffKind::WealthAcquisitionPotion => &*WEALTH_ACQUISITION_POTION_BUFF,
        BuffKind::ExpAccumulationPotion => &*EXP_ACCUMULATION_POTION_BUFF,
        BuffKind::SmallWealthAcquisitionPotion => &*SMALL_WEALTH_ACQUISITION_POTION_BUFF,
        BuffKind::SmallExpAccumulationPotion => &*SMALL_EXP_ACCUMULATION_POTION_BUFF,
        BuffKind::ForTheGuild => &*FOR_THE_GUILD_BUFF,
        BuffKind::HardHitter => &*HARD_HITTER_BUFF,
        BuffKind::ExtremeRedPotion => &*EXTREME_RED_POTION_BUFF,
        BuffKind::ExtremeBluePotion => &*EXTREME_BLUE_POTION_BUFF,
        BuffKind::ExtremeGreenPotion => &*EXTREME_GREEN_POTION_BUFF,
        BuffKind::ExtremeGoldPotion => &*EXTREME_GOLD_POTION_BUFF,
    };

    match kind {
        BuffKind::SmallWealthAcquisitionPotion
        | BuffKind::SmallExpAccumulationPotion
        | BuffKind::WealthAcquisitionPotion
        | BuffKind::ExpAccumulationPotion => {
            // Because the two potions are really similar, detecting one may mis-detect for the other.
            // Can't really think of a better way to do this.... But this seems working just fine.
            let mask = match kind {
                BuffKind::SmallWealthAcquisitionPotion | BuffKind::SmallExpAccumulationPotion => {
                    &*SMALL_WEALTH_EXP_POTION_MASK
                }
                BuffKind::WealthAcquisitionPotion | BuffKind::ExpAccumulationPotion => {
                    &*WEALTH_EXP_POTION_MASK
                }
                _ => unreachable!(),
            };
            let matches =
                detect_template_multiple(mat, template, mask, Point::default(), 2, threshold)
                    .into_iter()
                    .filter_map(|result| result.ok())
                    .collect::<Vec<_>>();
            if matches.is_empty() {
                return false;
            }
            // Likely both potions are active
            if matches.len() == 2 {
                return true;
            }

            let template_other = match kind {
                BuffKind::SmallWealthAcquisitionPotion => &*SMALL_EXP_ACCUMULATION_POTION_BUFF,
                BuffKind::SmallExpAccumulationPotion => &*SMALL_WEALTH_ACQUISITION_POTION_BUFF,
                BuffKind::WealthAcquisitionPotion => &*EXP_ACCUMULATION_POTION_BUFF,
                BuffKind::ExpAccumulationPotion => &*WEALTH_ACQUISITION_POTION_BUFF,
                _ => unreachable!(),
            };
            let match_current = matches.into_iter().next().unwrap();
            let match_other =
                detect_template_single(mat, template_other, mask, Point::default(), threshold);

            match_other.is_err()
                || match_other.as_ref().copied().unwrap().0 != match_current.0
                || match_other.unwrap().1 < match_current.1
        }
        BuffKind::LegionLuck => detect_template_single(
            mat,
            template,
            &*LEGION_LUCK_BUFF_MASK,
            Point::default(),
            threshold,
        )
        .is_ok(),
        BuffKind::LegionWealth => {
            detect_template_single(mat, template, no_array(), Point::default(), threshold)
                .or_else(|_| {
                    detect_template_single(
                        mat,
                        &*LEGION_WEALTH_BUFF_2,
                        no_array(),
                        Point::default(),
                        threshold,
                    )
                })
                .is_ok()
        }
        _ => detect_template(mat, template, Point::default(), threshold).is_ok(),
    }
}

fn detect_rune_arrows_with_scores_regions(bgr: &impl MatTraitConst) -> Vec<(Rect, KeyKind, f32)> {
    static RUNE_MODEL: LazyLock<Mutex<Session>> = LazyLock::new(|| {
        Mutex::new(
            build_session(include_bytes!(env!("RUNE_MODEL")))
                .expect("build rune detection session successfully"),
        )
    });

    fn map_arrow(pred: &[f32]) -> KeyKind {
        match pred[5] as i32 {
            0 => KeyKind::Up,
            1 => KeyKind::Down,
            2 => KeyKind::Left,
            3 => KeyKind::Right,
            _ => unreachable!(),
        }
    }

    let size = bgr.size().unwrap();
    let (mat_in, w_ratio, h_ratio, left, top) = preprocess_for_yolo(bgr);

    let mut model = RUNE_MODEL.lock().unwrap();
    let result = model.run([to_input_value(&mat_in)]).unwrap();

    let mat_out = from_output_value(&result);
    let mut vec = (0..mat_out.rows())
        // SAFETY: 0..outputs.rows() is within Mat bounds
        .map(|i| unsafe { mat_out.at_row_unchecked::<f32>(i).unwrap() })
        .filter(|pred| pred[4] >= 0.2)
        .map(|pred| {
            (
                remap_from_yolo(pred, size, w_ratio, h_ratio, left, top),
                map_arrow(pred),
                pred[4],
            )
        })
        .collect::<Vec<_>>();
    vec.sort_by_key(|a| a.0.x);
    vec
}

fn detect_rune_arrows(bgr: &impl MatTraitConst, ignore: Vec<Rect>) -> Result<Vec<Arrow>> {
    const SCORE_THRESHOLD: f32 = 0.8;

    let mut bgr = bgr.try_clone().unwrap();
    for region in ignore {
        bgr.roi_mut(region)?.set_scalar(Scalar::default())?;
    }

    Ok(detect_rune_arrows_with_scores_regions(&bgr)
        .into_iter()
        .filter_map(|(bbox, key, score)| {
            if score >= SCORE_THRESHOLD {
                Some(Arrow { key, region: bbox })
            } else {
                None
            }
        })
        .collect::<Vec<_>>())
}

fn detect_rune_initial_spin_arrows(bgr: &impl MatTraitConst) -> Vec<SpinArrow> {
    static RUNE_SPIN_MODEL: LazyLock<Mutex<Session>> = LazyLock::new(|| {
        Mutex::new(
            build_session(include_bytes!(env!("RUNE_SPIN_MODEL")))
                .expect("build rune spin detection session successfully"),
        )
    });

    const SPIN_REGION_PAD: i32 = 16;

    // Detect the rune region
    let size = bgr.size().unwrap();
    let (mat_in, w_ratio, h_ratio, left, top) = preprocess_for_yolo(bgr);
    let mut model = RUNE_SPIN_MODEL.lock().unwrap();
    let result = model.run([to_input_value(&mat_in)]).unwrap();
    let mat_out = from_output_value(&result);
    let spin_arrow_regions = (0..mat_out.rows())
        // SAFETY: 0..result.rows() is within Mat bounds
        .map(|i| unsafe { mat_out.at_row_unchecked::<f32>(i).unwrap() })
        .filter(|pred| pred[4] >= 0.8)
        .map(|pred| remap_from_yolo(pred, size, w_ratio, h_ratio, left, top))
        .collect::<Vec<Rect>>();

    spin_arrow_regions
        .into_iter()
        .map(|region| {
            // Pad to ensure the region always contain the spin arrow even when it rotates
            // horitzontally or vertically

            let x = region.x;
            let y = region.y;
            let padded_x = (x - SPIN_REGION_PAD).max(0);
            let padded_y = (y - SPIN_REGION_PAD).max(0);

            let w = region.width;
            let h = region.height;
            let padded_w = (padded_x + w + SPIN_REGION_PAD * 2).min(size.width) - padded_x;
            let padded_h = (padded_y + h + SPIN_REGION_PAD * 2).min(size.height) - padded_y;
            let rect = Rect::new(padded_x, padded_y, padded_w, padded_h);

            SpinArrow {
                centroid: Point::new(x + w / 2, y + h / 2),
                region: rect,
                last_arrow_head: None,
                final_arrow: None,
                #[cfg(debug_assertions)]
                last_last_arrow_head: None,
            }
        })
        .collect::<Vec<_>>()
}

fn detect_rune_spin_arrow(bgr: &impl MatTraitConst, spin_arrow: &mut SpinArrow) -> Result<()> {
    const SPIN_LAG_THRESHOLD: i32 = 30;

    // Extract spin arrow region
    let spin_arrow_mat = to_hsv(&bgr.roi(spin_arrow.region)?);
    let kernel = get_structuring_element_def(MORPH_RECT, Size::new(3, 3)).unwrap();
    let mut spin_arrow_thresh = Mat::default();
    unsafe {
        spin_arrow_thresh.modify_inplace(|mat, mat_mut| {
            extract_channel(&spin_arrow_mat, mat_mut, 1).unwrap();
            threshold(mat, mat_mut, 245.0, 255.0, THRESH_BINARY).unwrap();
            dilate_def(mat, mat_mut, &kernel).unwrap();
        })
    }

    let mut contours = Vector::<Vector<Point>>::new();
    find_contours_def(
        &spin_arrow_thresh,
        &mut contours,
        RETR_EXTERNAL,
        CHAIN_APPROX_SIMPLE,
    )
    .unwrap();
    if contours.is_empty() {
        bail!("cannot find the spinning arrow contour");
    }

    let contour = contours
        .iter()
        .min_by_key(|contour| contour_area(contour, false).unwrap() as i32)
        .expect("not empty");
    let mut triangle = Vector::<Point>::new();
    let triangle_area = min_enclosing_triangle(&contour, &mut triangle).unwrap() as i32;
    if triangle_area == 0 {
        bail!("failed to determine the spinning arrow triangle");
    }

    let shortest_edge = triangle
        .iter()
        .zip(triangle.iter().cycle().skip(1))
        .min_by(|first_edge, second_edge| {
            let first_norm = (first_edge.0 - first_edge.1).norm();
            let second_norm = (second_edge.0 - second_edge.1).norm();
            first_norm.total_cmp(&second_norm)
        })
        .expect("has value");
    let arrow_head = (shortest_edge.0 + shortest_edge.1) / 2;

    let centroid = spin_arrow.centroid - spin_arrow.region.tl();
    let cur_arrow_head = arrow_head - centroid;

    if spin_arrow.last_arrow_head.is_none() {
        #[cfg(debug_assertions)]
        {
            spin_arrow.last_last_arrow_head = Some(cur_arrow_head);
        }
        spin_arrow.last_arrow_head = Some(cur_arrow_head);
        return Ok(());
    }

    let prev_arrow_head = spin_arrow.last_arrow_head.unwrap();
    // https://stackoverflow.com/a/13221874
    let dot = prev_arrow_head.x * -cur_arrow_head.y + prev_arrow_head.y * cur_arrow_head.x;
    if dot >= SPIN_LAG_THRESHOLD {
        debug!(target: "backend/rune", "spinning arrow lag detected");
        let directions = [
            (KeyKind::Up, prev_arrow_head.dot(Point::new(0, -1))),
            (KeyKind::Down, prev_arrow_head.dot(Point::new(0, 1))),
            (KeyKind::Left, prev_arrow_head.dot(Point::new(-1, 0))),
            (KeyKind::Right, prev_arrow_head.dot(Point::new(1, 0))),
        ];
        let (arrow, _) = directions
            .into_iter()
            .max_by_key(|(_, score)| *score)
            .unwrap();
        info!(target: "backend/rune", "spinning arrow result {arrow:?} {directions:?}");
        spin_arrow.final_arrow = Some(arrow);
    }
    #[cfg(debug_assertions)]
    {
        spin_arrow.last_last_arrow_head = spin_arrow.last_arrow_head;
    }
    spin_arrow.last_arrow_head = Some(cur_arrow_head);

    Ok(())
}

fn detect_erda_shower(grayscale: &impl MatTraitConst) -> Result<Rect> {
    /// TODO: Support default ratio
    static ERDA_SHOWER: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("ERDA_SHOWER_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    let (quick_slots, crop_bbox) = to_quick_slots_region(grayscale);
    detect_template(&quick_slots, &*ERDA_SHOWER, crop_bbox.tl(), 0.8)
}

pub static FAMILIAR_SAVE_BUTTON_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("FAMILIAR_BUTTON_SAVE_TEMPLATE")),
        IMREAD_COLOR,
    )
    .unwrap()
});

fn detect_familiar_save_button(
    bgr: &impl ToInputArray,
    localization: &Localization,
) -> Result<Rect> {
    let template = localization
        .familiar_save_button_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, false).ok());

    detect_template(
        bgr,
        template.as_ref().unwrap_or(&*FAMILIAR_SAVE_BUTTON_TEMPLATE),
        Point::default(),
        0.75,
    )
}

pub static FAMILIAR_LEVEL_BUTTON_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("FAMILIAR_BUTTON_LEVEL_TEMPLATE")),
        IMREAD_COLOR,
    )
    .unwrap()
});

fn detect_familiar_level_button(
    bgr: &impl ToInputArray,
    localization: &Localization,
) -> Result<Rect> {
    let template = localization
        .familiar_level_button_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, false).ok());

    detect_template(
        bgr,
        template
            .as_ref()
            .unwrap_or(&*FAMILIAR_LEVEL_BUTTON_TEMPLATE),
        Point::default(),
        0.75,
    )
}

static FAMILIAR_SLOT_FREE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("FAMILIAR_SLOT_FREE_TEMPLATE")),
        IMREAD_COLOR,
    )
    .unwrap()
});

fn detect_familiar_slots(bgr: &impl ToInputArray) -> Vec<(Rect, bool)> {
    static FAMILIAR_SLOT_OCCUPIED: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("FAMILIAR_SLOT_OCCUPIED_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static FAMILIAR_SLOT_OCCUPIED_MASK: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("FAMILIAR_SLOT_OCCUPIED_MASK_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    let first = detect_template_multiple(
        bgr,
        &*FAMILIAR_SLOT_FREE,
        no_array(),
        Point::default(),
        3,
        0.75,
    );
    let first_slots = first
        .into_iter()
        .filter_map(|bbox| bbox.ok().map(|(bbox, _)| (bbox, true)))
        .collect::<Vec<(Rect, bool)>>();

    // The occupied slots detection acts more like a general familiar slot frame detection. This
    // may include both free and occupied slots. As such, truely free slots need to be filtered
    // out.
    let second = detect_template_multiple(
        bgr,
        &*FAMILIAR_SLOT_OCCUPIED,
        &*FAMILIAR_SLOT_OCCUPIED_MASK,
        Point::default(),
        3,
        0.75,
    );
    let second_slots = second
        .into_iter()
        .filter_map(|bbox| bbox.ok().map(|(bbox, _)| (bbox, false)))
        .filter(|(second_slot, _)| {
            !first_slots
                .iter()
                .any(|(first_slot, _)| iou(*first_slot, *second_slot) >= 0.8)
        })
        .collect::<Vec<(Rect, bool)>>();

    let mut vec = [first_slots, second_slots].concat();
    vec.sort_by_key(|(bbox, _)| bbox.x);
    vec
}

fn detect_familiar_slot_is_free(bgr: &impl ToInputArray) -> bool {
    detect_template(bgr, &*FAMILIAR_SLOT_FREE, Point::default(), 0.75).is_ok()
}

fn detect_familiar_hover_level<T: ToInputArray + MatTraitConst>(bgr: &T) -> Result<FamiliarLevel> {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("FAMILIAR_LEVEL_5_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static TEMPLATE_MASK: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("FAMILIAR_LEVEL_5_MASK_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    let level_bbox = detect_template(bgr, &*TEMPLATE, Point::default(), 0.65)?;
    let level = bgr.roi(level_bbox)?;
    Ok(
        detect_template_single(&level, &*TEMPLATE, &*TEMPLATE_MASK, Point::default(), 0.70)
            .map_or(FamiliarLevel::LevelOther, |_| FamiliarLevel::Level5),
    )
}

fn detect_familiar_cards<T: MatTraitConst + ToInputArray>(bgr: &T) -> Vec<(Rect, FamiliarRank)> {
    static TEMPLATE_RARE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("FAMILIAR_CARD_RARE_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static TEMPLATE_EPIC: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("FAMILIAR_CARD_EPIC_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static TEMPLATE_MASK: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("FAMILIAR_CARD_MASK_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    #[inline]
    fn match_template_score(
        mat: &impl ToInputArray,
        template: &impl ToInputArray,
        mask: &impl ToInputArray,
    ) -> f64 {
        let mut result = Mat::default();
        let mut score = 0f64;
        match_template(mat, template, &mut result, TM_SQDIFF_NORMED, mask).unwrap();
        min_max_loc(&result, Some(&mut score), None, None, None, &no_array()).unwrap();
        score
    }

    // The current method would match all card without distinguishing rarity
    let cards = detect_template_multiple(
        bgr,
        &*TEMPLATE_RARE,
        &*TEMPLATE_MASK,
        Point::default(),
        64,
        0.75,
    )
    .into_iter()
    .filter_map(|result| result.ok().map(|(bbox, _)| bbox))
    .collect::<Vec<_>>();

    let mut filtered = vec![];
    if cards.is_empty() {
        return filtered;
    }

    for card in cards {
        let roi = bgr.roi(card).unwrap();
        let score_rare = match_template_score(&roi, &*TEMPLATE_RARE, &*TEMPLATE_MASK);
        let score_epic = match_template_score(&roi, &*TEMPLATE_EPIC, &*TEMPLATE_MASK);
        // TODO: If matching all rarities, it will probably be easier since just need to
        // pick lowest score
        if score_rare < 0.14 || score_epic < 0.14 {
            let rank = if score_rare < score_epic {
                FamiliarRank::Rare
            } else {
                FamiliarRank::Epic
            };
            filtered.push((card, rank));
        }
    }
    filtered.sort_by_key(|(bbox, _)| (bbox.y, bbox.x));

    filtered
}

fn detect_familiar_scrollbar(grayscale: &impl ToInputArray) -> Result<Rect> {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("FAMILIAR_SCROLLBAR_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    detect_template(grayscale, &*TEMPLATE, Point::default(), 0.6)
}

fn detect_familiar_menu_opened(grayscale: &impl ToInputArray) -> bool {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("FAMILIAR_MENU_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    detect_template(grayscale, &*TEMPLATE, Point::default(), 0.75).is_ok()
}

fn detect_familiar_essence_depleted(grayscale: &impl ToInputArray) -> bool {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("FAMILIAR_ESSENCE_DEPLETE_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    detect_template(grayscale, &*TEMPLATE, Point::default(), 0.8).is_ok()
}

pub static CHANGE_CHANNEL_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("CHANGE_CHANNEL_MENU_TEMPLATE")),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

fn detect_change_channel_menu_opened(
    grayscale: &impl ToInputArray,
    localization: &Localization,
) -> bool {
    let template = localization
        .change_channel_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, true).ok());

    detect_template(
        grayscale,
        template.as_ref().unwrap_or(&*CHANGE_CHANNEL_TEMPLATE),
        Point::default(),
        0.75,
    )
    .is_ok()
}

fn detect_admin_visible(grayscale: &impl ToInputArray) -> bool {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("ADMIN_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    detect_template(grayscale, &*TEMPLATE, Point::default(), 0.75).is_ok()
}

pub static TIMER_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(include_bytes!(env!("TIMER_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
});

fn detect_timer_visible(grayscale: &impl ToInputArray, localization: &Localization) -> bool {
    let template = localization
        .timer_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, true).ok());

    detect_template(
        grayscale,
        template.as_ref().unwrap_or(&*TIMER_TEMPLATE),
        Point::default(),
        0.75,
    )
    .is_ok()
}

fn detect_lie_detector_shape(bgr: &impl ToInputArray) -> Result<Rect> {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LIE_DETECTOR_SHAPE_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });

    detect_template(bgr, &*TEMPLATE, Point::default(), 0.6)
}

fn detect_lie_detector_shape_preparing(bgr: &impl ToInputArray) -> Result<Rect> {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LIE_DETECTOR_SHAPE_PREPARE_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });

    detect_template(bgr, &*TEMPLATE, Point::default(), 0.6)
}

fn detect_lie_detector_violetta(bgr: &impl ToInputArray) -> Result<Rect> {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LIE_DETECTOR_VIOLETTA_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });

    detect_template(bgr, &*TEMPLATE, Point::default(), 0.6)
}

#[allow(unused)]
fn detect_lie_detector_violetta_preparing(bgr: &impl ToInputArray) -> Result<Rect> {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LIE_DETECTOR_VIOLETTA_PREPARE_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });

    detect_template(bgr, &*TEMPLATE, Point::default(), 0.6)
}

fn detect_quick_slots_hexa_booster<T: MatTraitConst + ToInputArray>(
    grayscale: &T,
) -> Result<QuickSlotsHexaBooster> {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HEXA_BOOSTER_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static TEMPLATE_NUMBER: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HEXA_BOOSTER_NUMBER_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static TEMPLATE_NUMBER_MASK: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HEXA_BOOSTER_NUMBER_MASK_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    let pad_height = TEMPLATE_NUMBER.size().unwrap().height;
    let booster_bbox =
        detect_template(grayscale, &*TEMPLATE, Point::default(), 0.75).map(|bbox| {
            let br = bbox.br();

            let x1 = bbox.x - 1;
            let x2 = br.x + 1;

            let y1 = bbox.y;
            let y2 = br.y + pad_height;

            Rect::new(x1, y1, x2 - x1, y2 - y1)
        })?;
    let booster = grayscale.roi(booster_bbox).expect("can extract roi");
    let has_booster = detect_template_single(
        &booster,
        &*TEMPLATE_NUMBER,
        &*TEMPLATE_NUMBER_MASK,
        Point::default(),
        0.8,
    )
    .is_err();

    if has_booster {
        Ok(QuickSlotsHexaBooster::Available)
    } else {
        Ok(QuickSlotsHexaBooster::Unavailable)
    }
}

fn detect_hexa_menu(grayscale: &impl ToInputArray) -> bool {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("HEXA_MENU_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    detect_template(grayscale, &*TEMPLATE, Point::default(), 0.75).is_ok()
}

fn detect_hexa_quick_menu(grayscale: &impl ToInputArray) -> Result<Rect> {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HEXA_QUICK_MENU_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    detect_template(grayscale, &*TEMPLATE, Point::default(), 0.75)
}

pub static HEXA_ERDA_CONVERSION_BUTTON_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("HEXA_BUTTON_ERDA_CONVERSION_TEMPLATE")),
        IMREAD_COLOR,
    )
    .unwrap()
});

fn detect_hexa_erda_conversion_button(
    bgr: &impl ToInputArray,
    localization: &Localization,
) -> Result<Rect> {
    let template = localization
        .hexa_erda_conversion_button_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, false).ok());

    detect_template(
        bgr,
        template
            .as_ref()
            .unwrap_or(&*HEXA_ERDA_CONVERSION_BUTTON_TEMPLATE),
        Point::default(),
        0.75,
    )
}

pub static HEXA_BOOSTER_BUTTON_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("HEXA_BUTTON_HEXA_BOOSTER_TEMPLATE")),
        IMREAD_COLOR,
    )
    .unwrap()
});

fn detect_hexa_booster_button(
    bgr: &impl ToInputArray,
    localization: &Localization,
) -> Result<Rect> {
    let template = localization
        .hexa_booster_button_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, false).ok());

    detect_template(
        bgr,
        template.as_ref().unwrap_or(&*HEXA_BOOSTER_BUTTON_TEMPLATE),
        Point::default(),
        0.75,
    )
}

pub static HEXA_MAX_BUTTON_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("HEXA_BUTTON_MAX_TEMPLATE")),
        IMREAD_COLOR,
    )
    .unwrap()
});

fn detect_hexa_max_button(bgr: &impl ToInputArray, localization: &Localization) -> Result<Rect> {
    let template = localization
        .hexa_max_button_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, false).ok());

    detect_template(
        bgr,
        template.as_ref().unwrap_or(&*HEXA_MAX_BUTTON_TEMPLATE),
        Point::default(),
        0.75,
    )
}

pub static HEXA_CONVERT_BUTTON_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("HEXA_BUTTON_CONVERT_TEMPLATE")),
        IMREAD_COLOR,
    )
    .unwrap()
});

fn detect_hexa_convert_button(
    bgr: &impl ToInputArray,
    localization: &Localization,
) -> Result<Rect> {
    let template = localization
        .hexa_convert_button_base64
        .as_ref()
        .and_then(|base64| to_mat_from_base64(base64, false).ok());

    detect_template(
        bgr,
        template.as_ref().unwrap_or(&*HEXA_CONVERT_BUTTON_TEMPLATE),
        Point::default(),
        0.75,
    )
}

fn detect_hexa_sol_erda(grayscale: &impl ToInputArray) -> Result<SolErda> {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HEXA_SOL_ERDA_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static FULL_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HEXA_SOL_ERDA_FULL_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static FULL_MASK_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HEXA_SOL_ERDA_FULL_MASK_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static EMPTY_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HEXA_SOL_ERDA_EMPTY_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static EMPTY_MASK_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HEXA_SOL_ERDA_EMPTY_MASK_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    if detect_template_single(
        grayscale,
        &*FULL_TEMPLATE,
        &*FULL_MASK_TEMPLATE,
        Point::default(),
        0.8,
    )
    .is_ok()
    {
        return Ok(SolErda::Full);
    }

    if detect_template_single(
        grayscale,
        &*EMPTY_TEMPLATE,
        &*EMPTY_MASK_TEMPLATE,
        Point::default(),
        0.8,
    )
    .is_ok()
    {
        return Ok(SolErda::Empty);
    }

    if detect_template(grayscale, &*TEMPLATE, Point::default(), 0.75).is_ok() {
        return Ok(SolErda::AtLeastOne);
    }

    bail!("sol erda tracker menu not visible")
}

fn detect_transparent_shapes(bgr: &impl MatTraitConst) -> Vec<(Rect, f32)> {
    static MODEL: LazyLock<Mutex<Session>> = LazyLock::new(|| {
        Mutex::new(
            build_session(include_bytes!(env!("TRANSPARENT_SHAPE_MODEL")))
                .expect("build transparent shape detection session successfully"),
        )
    });

    let size = bgr.size().unwrap();
    let (mat_in, w_ratio, h_ratio, left, top) = preprocess_for_yolo(bgr);
    let mut model = MODEL.lock().unwrap();
    let result = model.run([to_input_value(&mat_in)]).unwrap();
    let mat_out = from_output_value(&result);

    (0..mat_out.rows())
        // SAFETY: 0..result.rows() is within Mat bounds
        .map(|i| unsafe { mat_out.at_row_unchecked::<f32>(i).unwrap() })
        .map(|pred| {
            (
                remap_from_yolo(pred, size, w_ratio, h_ratio, left, top),
                pred[4],
            )
        })
        .collect()
}

fn detect_violetta_mushrooms(bgr: &impl MatTraitConst) -> Vec<(Rect, f32)> {
    static MODEL: LazyLock<Mutex<Session>> = LazyLock::new(|| {
        Mutex::new(
            build_session(include_bytes!(env!("VIOLETTA_MODEL")))
                .expect("build violetta detection session successfully"),
        )
    });

    let size = bgr.size().unwrap();
    let (mat_in, w_ratio, h_ratio, left, top) = preprocess_for_yolo(bgr);
    let mut model = MODEL.lock().unwrap();
    let result = model.run([to_input_value(&mat_in)]).unwrap();
    let mat_out = from_output_value(&result);

    (0..mat_out.rows())
        // SAFETY: 0..result.rows() is within Mat bounds
        .map(|i| unsafe { mat_out.at_row_unchecked::<f32>(i).unwrap() })
        .map(|pred| {
            (
                remap_from_yolo(pred, size, w_ratio, h_ratio, left, top),
                pred[4],
            )
        })
        .collect()
}

fn detect_violetta_face(bgr: &impl ToInputArray) -> Result<Rect> {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LIE_DETECTOR_VIOLETTA_FACE_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });

    detect_template(bgr, &*TEMPLATE, Point::default(), 0.6)
}

fn detect_violetta_numbers(bgr: &impl ToInputArray) -> Vec<Rect> {
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LIE_DETECTOR_VIOLETTA_NUMBER_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });

    static TEMPLATE_MASK: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LIE_DETECTOR_VIOLETTA_NUMBER_MASK_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    let mut vec =
        detect_template_multiple(bgr, &*TEMPLATE, &*TEMPLATE_MASK, Point::default(), 4, 0.75)
            .into_iter()
            .filter_map(|result| Some(result.ok()?.0))
            .collect::<Vec<Rect>>();
    if vec.is_empty() || vec.len() != 4 {
        return vec![];
    }

    vec.sort_by_key(|rect| rect.x);
    vec
}

/// Detects a single match from `template` with the given BGR image `Mat`.
#[inline]
fn detect_template<T: ToInputArray + MatTraitConst>(
    mat: &impl ToInputArray,
    template: &T,
    offset: Point,
    threshold: f64,
) -> Result<Rect> {
    detect_template_single(mat, template, no_array(), offset, threshold).map(|(bbox, _)| bbox)
}

/// Detects a single match with `mask` from `template` with the given BGR image `Mat`.
#[inline]
fn detect_template_single<T: ToInputArray + MatTraitConst>(
    mat: &impl ToInputArray,
    template: &T,
    mask: impl ToInputArray,
    offset: Point,
    threshold: f64,
) -> Result<(Rect, f64)> {
    detect_template_multiple(mat, template, mask, offset, 1, threshold)
        .into_iter()
        .next()
        .ok_or(anyhow!("no match"))
        .and_then(|x| x)
}

/// Detects multiple matches from `template` from the given BGR image `Mat` and returns up to
/// `max_matches` best results.
#[inline]
fn detect_template_multiple<T: ToInputArray + MatTraitConst>(
    mat: &impl ToInputArray,
    template: &T,
    mask: impl ToInputArray,
    offset: Point,
    max_matches: usize,
    threshold: f64,
) -> Vec<Result<(Rect, f64)>> {
    #[inline]
    fn clear_result(result: &mut Mat, loc: Point, template_size: Size) {
        let cols = result.cols();
        let rows = result.rows();

        let x1 = loc.x.clamp(0, cols);
        let y1 = loc.y.clamp(0, rows);
        let x2 = (loc.x + template_size.width).clamp(0, cols);
        let y2 = (loc.y + template_size.height).clamp(0, rows);

        if x2 <= x1 || y2 <= y1 {
            return;
        }

        result
            .roi_mut(Rect::new(x1, y1, x2 - x1, y2 - y1))
            .expect("valid ROI")
            .set_scalar(Scalar::all(0.0))
            .expect("failed to set scalar");
    }

    #[inline]
    fn append_result(
        matches: &mut Vec<Result<(Rect, f64)>>,
        score: f64,
        loc: Point,
        offset: Point,
        template_size: Size,
    ) {
        // Weird INFINITY values when match template with mask
        // https://github.com/opencv/opencv/issues/23257
        if score == f64::INFINITY {
            return;
        }

        let tl = Point::new(loc.x + offset.x, loc.y + offset.y);
        let br = tl + Point::new(template_size.width, template_size.height);
        let rect = Rect::from_points(tl, br);
        if matches
            .iter()
            .filter_map(|result| result.as_ref().ok())
            .any(|(match_rect, _)| iou(*match_rect, rect) > 0.9)
        {
            return;
        }

        matches.push(Ok((rect, score)));
    }

    let mut result = Mat::default();
    if let Err(err) = match_template(mat, template, &mut result, TM_CCOEFF_NORMED, &mask) {
        error!(target: "backend/detect", "template detection error {err}");
        return vec![];
    }

    let template_size = template.size().unwrap();
    let max_matches = max_matches.max(1);
    let mut matches = Vec::new();
    while matches.len() < max_matches {
        let mut score = 0f64;
        let mut loc = Point::default();
        min_max_loc(
            &result,
            None,
            Some(&mut score),
            None,
            Some(&mut loc),
            &no_array(),
        )
        .unwrap();
        if score < threshold {
            matches.push(Err(anyhow!("template not found").context(score)));
            break;
        }

        clear_result(&mut result, loc, template_size);
        append_result(&mut matches, score, loc, offset, template_size);
    }

    matches
}

/// Extracts texts from the non-preprocessed BGR `Mat` and detected text bounding boxes.
fn extract_texts(mat: &impl MatTraitConst, bboxes: &[Rect]) -> Vec<String> {
    static MODEL: LazyLock<Mutex<Session>> = LazyLock::new(|| {
        Mutex::new(
            build_session(include_bytes!(env!("TEXT_RECOGNITION_MODEL")))
                .expect("build text recognition session normally"),
        )
    });

    static ALPHABET: LazyLock<String> = LazyLock::new(|| {
        include_str!(env!("TEXT_RECOGNITION_ALPHABET"))
            .split("\n")
            .collect()
    });

    fn ctc_greedy_decode(mat: Mat) -> String {
        let mut result = String::new();
        let blank_class = 0usize;
        let mut prev_class = blank_class;

        for timestep in 0..mat.rows() {
            let logits = mat.row(timestep).unwrap();
            let class = logits
                .iter::<f32>()
                .unwrap()
                .enumerate()
                .max_by(|(_, (_, first)), (_, (_, second))| first.partial_cmp(second).unwrap())
                .map(|(class, _)| class)
                .unwrap();

            if class != blank_class && class != prev_class {
                result.push(ALPHABET.chars().nth(class - 1).unwrap());
            }

            prev_class = class;
        }

        result
    }

    let mut model = MODEL.lock().unwrap();

    bboxes
        .iter()
        .filter_map(|&bbox| {
            let roi = mat.roi(bbox).ok()?;
            let input = preprocess_for_text(&roi);
            Some(to_input_value(&input))
        })
        .map(|input| {
            let result = model.run([input]).unwrap();
            let mat = from_output_value_with_batch_index(&result, 1);
            ctc_greedy_decode(mat)
        })
        .collect::<Vec<_>>()
}

/// Extracts text bounding boxes from the preprocessed [`Mat`].
///
/// This function is adapted from
/// https://github.com/clovaai/CRAFT-pytorch/blob/master/craft_utils.py#L19 with minor changes
fn extract_text_bboxes(
    mat_in: &impl MatTraitConst,
    w_ratio: f32,
    h_ratio: f32,
    x_offset: i32,
    y_offset: i32,
) -> Vec<Rect> {
    const TEXT_SCORE_THRESHOLD: f64 = 0.7;
    const LINK_SCORE_THRESHOLD: f64 = 0.4;
    static TEXT_DETECTION_MODEL: LazyLock<Mutex<Session>> = LazyLock::new(|| {
        Mutex::new(
            build_session(include_bytes!(env!("TEXT_DETECTION_MODEL")))
                .expect("build text detection session normally"),
        )
    });

    let mut model = TEXT_DETECTION_MODEL.lock().unwrap();
    let result = model.run([to_input_value(mat_in)]).unwrap();
    let mat = from_output_value(&result);
    let text_score = mat
        .ranges(&Vector::from_iter([
            Range::all().unwrap(),
            Range::all().unwrap(),
            Range::new(0, 1).unwrap(),
        ]))
        .unwrap()
        .clone_pointee();
    // remove last channel (not sure what other way to do it without clone_pointee first)
    let text_score = text_score
        .reshape_nd(1, &text_score.mat_size()[..2])
        .unwrap();

    let mut text_low_score = Mat::default();
    threshold(
        &text_score,
        &mut text_low_score,
        LINK_SCORE_THRESHOLD,
        1.0,
        THRESH_BINARY,
    )
    .unwrap();

    let mut link_score = mat
        .ranges(&Vector::from_iter([
            Range::all().unwrap(),
            Range::all().unwrap(),
            Range::new(1, 2).unwrap(),
        ]))
        .unwrap()
        .clone_pointee();
    // remove last channel (not sure what other way to do it without clone_pointee first)
    let link_score_size = link_score.mat_size().to_vec();
    let mut link_score = link_score.reshape_nd_mut(1, &link_score_size[..2]).unwrap();
    // SAFETY: can be modified in place
    unsafe {
        link_score.modify_inplace(|mat, mat_mut| {
            threshold(mat, mat_mut, LINK_SCORE_THRESHOLD, 1.0, THRESH_BINARY).unwrap();
        });
    }

    let mut combined_score = Mat::default();
    let mut gt_one_mask = Mat::default();
    add(
        &text_low_score,
        &link_score,
        &mut combined_score,
        &no_array(),
        CV_8U,
    )
    .unwrap();
    compare(&combined_score, &Scalar::all(1.0), &mut gt_one_mask, CMP_GT).unwrap();
    combined_score
        .set_to(&Scalar::all(1.0), &gt_one_mask)
        .unwrap();

    let mut bboxes = Vec::<Rect>::new();
    let mut labels = Mat::default();
    let mut stats = Mat::default();
    let labels_count = connected_components_with_stats(
        &combined_score,
        &mut labels,
        &mut stats,
        &mut Mat::default(),
        4,
        CV_32S,
    )
    .unwrap();
    for i in 1..labels_count {
        let area = *stats.at_2d::<i32>(i, CC_STAT_AREA).unwrap();
        if area < 10 {
            continue;
        }
        let mut mask = Mat::default();
        let mut max_score = 0.0f64;
        compare(&labels, &Scalar::all(i as f64), &mut mask, CMP_EQ).unwrap();
        min_max_loc(&text_score, None, Some(&mut max_score), None, None, &mask).unwrap();
        if max_score < TEXT_SCORE_THRESHOLD {
            continue;
        }

        let shape = mask.size().unwrap();
        // SAFETY: The position (row, col) is guaranteed by OpenCV
        let x = unsafe { *stats.at_2d_unchecked::<i32>(i, CC_STAT_LEFT).unwrap() };
        let y = unsafe { *stats.at_2d_unchecked::<i32>(i, CC_STAT_TOP).unwrap() };
        let w = unsafe { *stats.at_2d_unchecked::<i32>(i, CC_STAT_WIDTH).unwrap() };
        let h = unsafe { *stats.at_2d_unchecked::<i32>(i, CC_STAT_HEIGHT).unwrap() };
        let size = area as f64 * w.min(h) as f64 / (w as f64 * h as f64);
        let size = ((size).sqrt() * 2.0) as i32;
        let sx = (x - size + 1).max(0);
        let sy = (y - size + 1).max(0);
        let ex = (x + w + size + 1).min(shape.width);
        let ey = (y + h + size + 1).min(shape.height);
        let kernel =
            get_structuring_element_def(MORPH_RECT, Size::new(size + 1, size + 1)).unwrap();

        let mut link_mask = Mat::default();
        let mut text_mask = Mat::default();
        let mut and_mask = Mat::default();
        let mut seg_map = Mat::zeros(shape.height, shape.width, CV_8U)
            .unwrap()
            .to_mat()
            .unwrap();
        compare(&link_score, &Scalar::all(1.0), &mut link_mask, CMP_EQ).unwrap();
        compare(&text_score, &Scalar::all(0.0), &mut text_mask, CMP_EQ).unwrap();
        bitwise_and_def(&link_mask, &text_mask, &mut and_mask).unwrap();
        seg_map.set_to(&Scalar::all(255.0), &mask).unwrap();
        seg_map.set_to(&Scalar::all(0.0), &and_mask).unwrap();

        let mut seg_contours = Vector::<Point>::new();
        let mut seg_roi = seg_map
            .roi_mut(Rect::from_points(Point::new(sx, sy), Point::new(ex, ey)))
            .unwrap();
        // SAFETY: all of the functions below can be called in place.
        unsafe {
            seg_roi.modify_inplace(|mat, mat_mut| {
                dilate_def(mat, mat_mut, &kernel).unwrap();
                mat.copy_to(mat_mut).unwrap();
            });
        }
        find_non_zero(&seg_map, &mut seg_contours).unwrap();

        let contour = min_area_rect(&seg_contours)
            .unwrap()
            .bounding_rect2f()
            .unwrap();
        let tl = contour.tl();
        let tl = Point::new(
            (tl.x * w_ratio * 2.0) as i32 + x_offset,
            (tl.y * h_ratio * 2.0) as i32 + y_offset,
        );
        let br = contour.br();
        let br = Point::new(
            (br.x * w_ratio * 2.0) as i32 + x_offset,
            (br.y * h_ratio * 2.0) as i32 + y_offset,
        );
        bboxes.push(Rect::from_points(tl, br));
    }
    bboxes
}

#[inline]
fn remap_from_yolo(
    pred: &[f32],
    size: Size,
    w_ratio: f32,
    h_ratio: f32,
    left: i32,
    top: i32,
) -> Rect {
    let tl_x = ((pred[0] - left as f32) / w_ratio)
        .max(0.0)
        .min(size.width as f32);
    let tl_y = ((pred[1] - top as f32) / h_ratio)
        .max(0.0)
        .min(size.height as f32);
    let br_x = ((pred[2] - left as f32) / w_ratio)
        .max(0.0)
        .min(size.width as f32);
    let br_y = ((pred[3] - top as f32) / h_ratio)
        .max(0.0)
        .min(size.height as f32);
    Rect::from_points(
        Point::new(tl_x as i32, tl_y as i32),
        Point::new(br_x as i32, br_y as i32),
    )
}

/// Preprocesses a BGR `Mat` image to a normalized and resized RGB `Mat` image with type `f32`
/// for YOLO detection.
///
/// Returns a triplet of `(Mat, width_ratio, height_ratio, left, top)`
#[inline]
fn preprocess_for_yolo(mat: &impl MatTraitConst) -> (Mat, f32, f32, i32, i32) {
    // https://github.com/ultralytics/ultralytics/blob/main/ultralytics/data/augment.py
    let mut mat = mat.try_clone().unwrap();

    let size = mat.size().unwrap();
    let (w_ratio, h_ratio) = (640.0 / size.width as f32, 640.0 / size.height as f32);
    let min_ratio = w_ratio.min(h_ratio);

    let w = (size.width as f32 * min_ratio).round();
    let h = (size.height as f32 * min_ratio).round();

    let pad_w = (640.0 - w) / 2.0;
    let pad_h = (640.0 - h) / 2.0;

    let top = (pad_h - 0.1).round() as i32;
    let bottom = (pad_h + 0.1).round() as i32;
    let left = (pad_w - 0.1).round() as i32;
    let right = (pad_w + 0.1).round() as i32;

    // SAFETY: all of the functions below can be called in place.
    unsafe {
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGR2RGB).unwrap();
            resize(
                mat,
                mat_mut,
                Size::new(w as i32, h as i32),
                0.0,
                0.0,
                INTER_LINEAR,
            )
            .unwrap();
            copy_make_border(
                mat,
                mat_mut,
                top,
                bottom,
                left,
                right,
                BORDER_CONSTANT,
                Scalar::all(114.0),
            )
            .unwrap();
            mat.convert_to(mat_mut, CV_32FC3, 1.0 / 255.0, 0.0).unwrap();
        });
    }
    (mat, min_ratio, min_ratio, left, top)
}

/// Preprocesses a BGR `Mat` image for text bounding boxes detection with `5.0` magnification ratio.
/// This function is meant for small images.
#[inline]
fn preprocess_for_text_bboxes(bgr: &impl MatTraitConst) -> (Mat, f32, f32) {
    preprocess_for_text_bboxes_magnified(bgr, 5.0)
}

/// Preprocesses a BGR `Mat` image to a normalized and resized RGB `Mat` image with type `f32`
/// for text bounding boxes detection.
///
/// The preprocess is adapted from: https://github.com/clovaai/CRAFT-pytorch/blob/master/imgproc.py
///
/// Returns a `(Mat, width_ratio, height_ratio)`.
fn preprocess_for_text_bboxes_magnified(
    bgr: &impl MatTraitConst,
    magnification_ratio: f32,
) -> (Mat, f32, f32) {
    let mut mat = bgr.try_clone().unwrap();
    let size = mat.size().unwrap();
    let size_w = size.width as f32;
    let size_h = size.height as f32;
    let size_max = size_w.max(size_h);
    let resize_size = magnification_ratio * size_max;
    let resize_ratio = resize_size / size_max;

    let resize_w = (resize_ratio * size_w) as i32;
    let resize_w = (resize_w + 31) & !31; // rounds to multiple of 32
    let resize_w_ratio = size_w / resize_w as f32;

    let resize_h = (resize_ratio * size_h) as i32;
    let resize_h = (resize_h + 31) & !31;
    let resize_h_ratio = size_h / resize_h as f32;
    // SAFETY: all of the below functions can be called in place
    unsafe {
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGR2RGB).unwrap();
            resize(
                mat,
                mat_mut,
                Size::new(resize_w, resize_h),
                0.0,
                0.0,
                INTER_CUBIC,
            )
            .unwrap();
            mat.convert_to(mat_mut, CV_32FC3, 1.0, 0.0).unwrap();
            // these values are pre-multiplied from the above link in normalizeMeanVariance
            subtract_def(mat, &Scalar::new(123.675, 116.28, 103.53, 0.0), mat_mut).unwrap();
            divide2_def(&mat, &Scalar::new(58.395, 57.12, 57.375, 1.0), mat_mut).unwrap();
        });
    }
    (mat, resize_w_ratio, resize_h_ratio)
}

/// Preprocesses a BGR `Mat` image to a normalized and resized RGB `Mat` image with type `f32`
/// for CRNN+CTC text recognition.
fn preprocess_for_text(bgr: &impl MatTraitConst) -> Mat {
    let mut mat = bgr.try_clone().unwrap();

    unsafe {
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGR2RGB).unwrap();
            resize(mat, mat_mut, Size::new(100, 32), 0.0, 0.0, INTER_LINEAR).unwrap();
            mat.convert_to(mat_mut, CV_32FC3, 1.0, 0.0).unwrap();
            subtract_def(&mat, &Scalar::all(127.5), mat_mut).unwrap();
            divide2_def(&mat, &Scalar::all(127.5), mat_mut).unwrap();
        });
    }

    mat
}

/// Expands `bbox` in all the direction by `count` pixel(s) and clamps to `size` if provided.
#[inline]
fn expand_bbox(size: Option<Size>, bbox: Rect, count: i32) -> Rect {
    let mut x1 = bbox.x - count;
    let mut y1 = bbox.y - count;
    if size.is_some() {
        x1 = x1.max(0);
        y1 = y1.max(0);
    }

    let br = bbox.br();
    let mut x2 = br.x + count;
    let mut y2 = br.y + count;
    if let Some(size) = size {
        x2 = x2.min(size.width);
        y2 = y2.min(size.height);
    }

    Rect::new(x1, y1, x2 - x1, y2 - y1)
}

/// Computes the intersection over union ratio.
#[inline]
fn iou(first: Rect, second: Rect) -> f32 {
    let intersection = (first & second).area() as f32;
    let union = (first | second).area() as f32;
    intersection / union
}

/// Crops `mat` to the buffs region.
#[inline]
fn to_buffs_region(mat: &impl MatTraitConst) -> BoxedRef<'_, Mat> {
    let size = mat.size().unwrap();
    // Crop to top right of the image for buffs region
    let crop_x = size.width / 3;
    let crop_y = size.height / 4;
    let crop_bbox = Rect::new(size.width - crop_x, 0, crop_x, crop_y);
    mat.roi(crop_bbox).unwrap()
}

/// Crops `mat` to the bottom right of the image for quick slots region.
#[inline]
fn to_quick_slots_region(mat: &impl MatTraitConst) -> (BoxedRef<'_, Mat>, Rect) {
    let size = mat.size().unwrap();
    let crop_x = size.width / 2;
    let crop_y = size.height / 5;
    let crop_bbox = Rect::new(size.width - crop_x, size.height - crop_y, crop_x, crop_y);
    let crop_roi = mat.roi(crop_bbox).unwrap();
    (crop_roi, crop_bbox)
}

/// Converts a BGR `Mat` image to HSV.
#[inline]
fn to_hsv(mat: &impl MatTraitConst) -> Mat {
    let mut mat = mat.try_clone().unwrap();
    unsafe {
        // SAFETY: can be modified inplace
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGR2HSV_FULL).unwrap();
        });
    }
    mat
}

/// Converts a BGRA `Mat` image to BGR.
#[inline]
fn to_bgr(mat: &impl MatTraitConst) -> Mat {
    let mut mat = mat.try_clone().unwrap();
    unsafe {
        // SAFETY: can be modified inplace
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2BGR).unwrap();
        });
    }
    mat
}

/// Converts a BGRA `Mat` image to grayscale.
///
/// `add_contrast` can be set to `true` in order to increase contrast by a fixed amount
/// used for template matching.
#[inline]
fn to_grayscale(mat: &impl MatTraitConst, add_contrast: bool) -> Mat {
    let mut mat = mat.try_clone().unwrap();
    unsafe {
        // SAFETY: all of the functions below can be called in place.
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2GRAY).unwrap();
            if add_contrast {
                // TODO: is this needed?
                add_weighted_def(mat, 1.5, mat, 0.0, -80.0, mat_mut).unwrap();
            }
        });
    }
    mat
}

/// Converts `base64` to a [`Mat`].
///
/// If `grayscale` is `true`, `base64` will be read with [`IMREAD_GRAYSCALE`]. Otherwise, it is
/// read with [`IMREAD_COLOR`].
fn to_mat_from_base64(base64: &str, grayscale: bool) -> Result<Mat> {
    let flag = if grayscale {
        IMREAD_GRAYSCALE
    } else {
        IMREAD_COLOR
    };
    let bytes = BASE64_STANDARD.decode(base64)?;
    let bytes = Vector::<u8>::from_iter(bytes);

    Ok(imdecode(&bytes, flag)?)
}

/// Converts `mat` to a base64 PNG [`String`].
pub fn to_base64_from_mat(mat: &Mat) -> Result<String> {
    let mut bytes = Vector::new();
    imencode_def(".png", mat, &mut bytes)?;
    Ok(BASE64_STANDARD.encode(bytes))
}

/// Extracts a `Mat` from `SessionOutputs` assuming batch index is 0.
///
/// The returned `Mat` has batch dimension removed.
#[inline]
fn from_output_value(result: &SessionOutputs) -> Mat {
    from_output_value_with_batch_index(result, 0)
}

/// Extracts a `Mat` from `SessionOutputs` and `batch_index`.
///
/// The returned `Mat` has batch dimension removed.
#[inline]
fn from_output_value_with_batch_index(result: &SessionOutputs, batch_index: usize) -> Mat {
    let key = result.keys().next().unwrap();
    let (dims, outputs) = result[key].try_extract_tensor::<f32>().unwrap();

    let dims = dims.iter().map(|&dim| dim as i32).collect::<Vec<i32>>();
    let mat = Mat::new_nd_with_data(dims.as_slice(), outputs).unwrap();

    let new_dims = dims
        .iter()
        .enumerate()
        .filter_map(|(i, &dim)| if i == batch_index { None } else { Some(dim) })
        .collect::<Vec<i32>>();
    let mat = mat.reshape_nd(1, &new_dims).unwrap();

    mat.clone_pointee()
}

/// Converts a continuous, normalized `f32` RGB `Mat` image to `SessionInputValue`.
///
/// The input `Mat` is assumed to be continuous, normalized RGB `f32` data type and
/// will panic if not. The `Mat` is reshaped to single channel, tranposed to `[1, 3, H, W]` and
/// converted to `SessionInputValue`.
#[inline]
fn to_input_value(mat: &impl MatTraitConst) -> SessionInputValue<'static> {
    let mat = mat.reshape_nd(1, &[1, mat.rows(), mat.cols(), 3]).unwrap();
    let mut mat_t = Mat::default();
    transpose_nd(&mat, &Vector::from_slice(&[0, 3, 1, 2]), &mut mat_t).unwrap();
    let shape = mat_t.mat_size();
    let input = (shape.to_vec(), mat_t.data_typed::<f32>().unwrap());
    let tensor = TensorRef::from_array_view(input).unwrap();
    SessionInputValue::Owned(tensor.clone().into_dyn())
}

#[inline]
fn build_session(model: &[u8]) -> Result<Session> {
    Ok(Session::builder()?
        .with_execution_providers([CUDAExecutionProvider::default().build()])?
        .commit_from_memory(model)?)
}
