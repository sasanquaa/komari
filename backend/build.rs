use std::process::Command;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

macro_rules! p {
    ($($tokens: tt)*) => {
        println!("cargo::warning={}", format!($($tokens)*))
    }
}

fn main() {
    let dir = env!("CARGO_MANIFEST_DIR").to_string();
    let dir = Path::new(&dir);
    let target_dir = dir.parent().unwrap().join("target");
    let resources_dir = dir.join("resources");

    let popup_yes = resources_dir.join("popup_yes_ideal_ratio.png");
    let popup_ok_old = resources_dir.join("popup_ok_old_ideal_ratio.png");
    let popup_ok_new = resources_dir.join("popup_ok_new_ideal_ratio.png");
    let popup_confirm = resources_dir.join("popup_confirm_ideal_ratio.png");
    let popup_cancel_old = resources_dir.join("popup_cancel_old_ideal_ratio.png");
    let popup_cancel_new = resources_dir.join("popup_cancel_new_ideal_ratio.png");
    let popup_end_chat = resources_dir.join("popup_end_chat_ideal_ratio.png");
    let popup_next = resources_dir.join("popup_next_ideal_ratio.png");

    let elite_boss_bar_1 = resources_dir.join("elite_boss_bar_1_ideal_ratio.png");
    let elite_boss_bar_2 = resources_dir.join("elite_boss_bar_2_ideal_ratio.png");

    let player = resources_dir.join("player_ideal_ratio.png");
    let player_left_half = resources_dir.join("player_left_half_ideal_ratio.png");
    let player_right_half = resources_dir.join("player_right_half_ideal_ratio.png");
    let player_top_half = resources_dir.join("player_top_half_ideal_ratio.png");
    let player_bottom_half = resources_dir.join("player_bottom_half_ideal_ratio.png");
    let player_stranger = resources_dir.join("player_stranger_ideal_ratio.png");
    let player_guildie = resources_dir.join("player_guildie_ideal_ratio.png");
    let player_friend = resources_dir.join("player_friend_ideal_ratio.png");

    let esc_menu = resources_dir.join("esc_menu_ideal_ratio.png");
    let tomb = resources_dir.join("tomb_ideal_ratio.png");
    let cash_shop = resources_dir.join("cash_shop.png");
    let erda_shower = resources_dir.join("erda_shower_ideal_ratio.png");
    let portal = resources_dir.join("portal_ideal_ratio.png");
    let change_channel_menu = resources_dir.join("change_channel_menu_ideal_ratio.png");
    let admin = resources_dir.join("admin_ideal_ratio.png");
    let timer = resources_dir.join("timer_ideal_ratio.png");
    let level = resources_dir.join("level_ideal_ratio.png");
    let lie_detector_shape = resources_dir.join("lie_detector_shape_ideal_ratio.png");
    let lie_detector_shape_prepare =
        resources_dir.join("lie_detector_shape_prepare_ideal_ratio.png");
    let lie_detector_violetta = resources_dir.join("lie_detector_violetta_ideal_ratio.png");
    let lie_detector_violetta_face =
        resources_dir.join("lie_detector_violetta_face_ideal_ratio.png");
    let lie_detector_violetta_prepare =
        resources_dir.join("lie_detector_violetta_prepare_ideal_ratio.png");
    let lie_detector_violetta_number =
        resources_dir.join("lie_detector_violetta_number_ideal_ratio.png");
    let lie_detector_violetta_number_mask =
        resources_dir.join("lie_detector_violetta_number_mask_ideal_ratio.png");

    let rune = resources_dir.join("rune_ideal_ratio.png");
    let rune_mask = resources_dir.join("rune_mask_ideal_ratio.png");
    let rune_buff = resources_dir.join("rune_buff_ideal_ratio.png");
    let spin_test = resources_dir.join("spin_test");
    let transparent_shape_test_normal = resources_dir.join("transparent_shape_test_normal.mp4");
    let transparent_shape_test_hard = resources_dir.join("transparent_shape_test_hard.mp4");
    let violetta_test = resources_dir.join("violetta_test.mp4");

    let sayram_elixir_buff = resources_dir.join("sayram_elixir_buff_ideal_ratio.png");
    let aurelia_elixir_buff = resources_dir.join("aurelia_elixir_buff_ideal_ratio.png");

    let exp_coupon_x2_buff = resources_dir.join("exp_coupon_x2_buff_ideal_ratio.png");
    let exp_coupon_x3_buff = resources_dir.join("exp_coupon_x3_buff_ideal_ratio.png");
    let exp_coupon_x4_buff = resources_dir.join("exp_coupon_x4_buff_ideal_ratio.png");
    let bonus_exp_coupon_buff = resources_dir.join("bonus_exp_coupon_buff_ideal_ratio.png");
    let mvp_bonus_exp_coupon_buff = resources_dir.join("mvp_bonus_exp_coupon_buff_ideal_ratio.png");

    let legion_wealth_buff = resources_dir.join("legion_wealth_buff_ideal_ratio.png");
    let legion_wealth_buff_2 = resources_dir.join("legion_wealth_buff_2_ideal_ratio.png");
    let legion_luck_buff = resources_dir.join("legion_luck_buff_ideal_ratio.png");
    let legion_luck_buff_mask = resources_dir.join("legion_luck_buff_mask_ideal_ratio.png");

    let wealth_acquisition_potion_buff =
        resources_dir.join("wealth_acquisition_potion_ideal_ratio.png");
    let wealth_exp_potion_mask = resources_dir.join("wealth_exp_potion_mask_ideal_ratio.png");
    let exp_accumulation_potion_buff =
        resources_dir.join("exp_accumulation_potion_ideal_ratio.png");

    let small_wealth_acquisition_potion_buff =
        resources_dir.join("small_wealth_acquisition_potion_ideal_ratio.png");
    let small_wealth_exp_potion_mask =
        resources_dir.join("small_wealth_exp_potion_mask_ideal_ratio.png");
    let small_exp_accumulation_potion_buff =
        resources_dir.join("small_exp_accumulation_potion_ideal_ratio.png");

    let for_the_guild_buff = resources_dir.join("for_the_guild_buff_ideal_ratio.png");
    let hard_hitter_buff = resources_dir.join("hard_hitter_buff_ideal_ratio.png");

    let extreme_red_potion_buff = resources_dir.join("extreme_red_potion_ideal_ratio.png");
    let extreme_blue_potion_buff = resources_dir.join("extreme_blue_potion_ideal_ratio.png");
    let extreme_green_potion_buff = resources_dir.join("extreme_green_potion_ideal_ratio.png");
    let extreme_gold_potion_buff = resources_dir.join("extreme_gold_potion_ideal_ratio.png");

    let hexa_booster = resources_dir.join("hexa_booster_ideal_ratio.png");
    let hexa_booster_number = resources_dir.join("hexa_booster_number_ideal_ratio.png");
    let hexa_booster_number_mask = resources_dir.join("hexa_booster_number_mask_ideal_ratio.png");

    let hexa_menu = resources_dir.join("hexa_menu_ideal_ratio.png");
    let hexa_quick_menu = resources_dir.join("hexa_quick_menu_ideal_ratio.png");
    let hexa_button_erda_conversion =
        resources_dir.join("hexa_button_erda_conversion_ideal_ratio.png");
    let hexa_button_hexa_booster = resources_dir.join("hexa_button_hexa_booster_ideal_ratio.png");
    let hexa_button_max = resources_dir.join("hexa_button_max_ideal_ratio.png");
    let hexa_button_convert = resources_dir.join("hexa_button_convert_ideal_ratio.png");
    let hexa_sol_erda = resources_dir.join("hexa_sol_erda_ideal_ratio.png");
    let hexa_sol_erda_full = resources_dir.join("hexa_sol_erda_full_ideal_ratio.png");
    let hexa_sol_erda_full_mask = resources_dir.join("hexa_sol_erda_full_mask_ideal_ratio.png");
    let hexa_sol_erda_empty = resources_dir.join("hexa_sol_erda_empty_ideal_ratio.png");
    let hexa_sol_erda_empty_mask = resources_dir.join("hexa_sol_erda_empty_mask_ideal_ratio.png");

    let hp_bar_anchor = resources_dir.join("hp_bar_anchor_ideal_ratio.png");
    let hp_separator = resources_dir.join("hp_separator_ideal_ratio.png");
    let hp_shield = resources_dir.join("hp_shield_ideal_ratio.png");

    let familiar_button_save = resources_dir.join("familiar_button_save_ideal_ratio.png");
    let familiar_button_setup = resources_dir.join("familiar_button_setup_ideal_ratio.png");
    let familiar_button_level = resources_dir.join("familiar_button_level_ideal_ratio.png");
    let familiar_slot_free = resources_dir.join("familiar_slot_free_ideal_ratio.png");
    let familiar_slot_occupied = resources_dir.join("familiar_slot_occupied_ideal_ratio.png");
    let familiar_slot_occupied_mask =
        resources_dir.join("familiar_slot_occupied_mask_ideal_ratio.png");
    let familiar_level_5 = resources_dir.join("familiar_level_5_ideal_ratio.png");
    let familiar_level_5_mask = resources_dir.join("familiar_level_5_mask_ideal_ratio.png");
    let familiar_scrollbar = resources_dir.join("familiar_scrollbar_ideal_ratio.png");
    let familiar_card_rare = resources_dir.join("familiar_card_rare_ideal_ratio.png");
    let familiar_card_epic = resources_dir.join("familiar_card_epic_ideal_ratio.png");
    let familiar_card_mask = resources_dir.join("familiar_card_mask_ideal_ratio.png");
    let familiar_buff = resources_dir.join("familiar_buff_ideal_ratio.png");
    let familiar_menu = resources_dir.join("familiar_menu_ideal_ratio.png");
    let familiar_essence_deplete = resources_dir.join("familiar_essence_deplete_ideal_ratio.png");

    let onnx_runtime = resources_dir.join("onnxruntime/onnxruntime.dll");
    let onnx_runtime_cuda = resources_dir.join("onnxruntime/onnxruntime_providers_cuda.dll");
    let onnx_runtime_shared = resources_dir.join("onnxruntime/onnxruntime_providers_shared.dll");

    let mob_model = resources_dir.join("mob_nms.onnx");
    let rune_model = resources_dir.join("rune_nms.onnx");
    let rune_spin_model = resources_dir.join("rune_spin_nms.onnx");
    let minimap_model = resources_dir.join("minimap_nms.onnx");
    let transparent_shape_model = resources_dir.join("transparent_shape_nms.onnx");
    let violetta_model = resources_dir.join("violetta_nms.onnx");
    let text_detection_model = resources_dir.join("text_detection.onnx");
    let text_recognition_model = resources_dir.join("text_recognition.onnx");
    let text_alphabet_txt = resources_dir.join("alphabet_94.txt");

    let proto_dir = dir.join("proto");
    let proto_file = proto_dir.join("input.proto");
    let out_dir = dir.join("src").join("grpc");
    tonic_build::configure()
        .out_dir(out_dir)
        .compile_protos(&[proto_file], &[proto_dir])
        .unwrap();

    println!(
        "cargo:rustc-env=POPUP_YES_TEMPLATE={}",
        popup_yes.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=POPUP_OK_OLD_TEMPLATE={}",
        popup_ok_old.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=POPUP_OK_NEW_TEMPLATE={}",
        popup_ok_new.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=POPUP_CONFIRM_TEMPLATE={}",
        popup_confirm.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=POPUP_CANCEL_OLD_TEMPLATE={}",
        popup_cancel_old.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=POPUP_CANCEL_NEW_TEMPLATE={}",
        popup_cancel_new.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=POPUP_END_CHAT_TEMPLATE={}",
        popup_end_chat.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=POPUP_NEXT_TEMPLATE={}",
        popup_next.to_str().unwrap()
    );

    println!(
        "cargo:rustc-env=ELITE_BOSS_BAR_1_TEMPLATE={}",
        elite_boss_bar_1.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ELITE_BOSS_BAR_2_TEMPLATE={}",
        elite_boss_bar_2.to_str().unwrap()
    );

    println!(
        "cargo:rustc-env=PLAYER_TEMPLATE={}",
        player.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=PLAYER_LEFT_HALF_TEMPLATE={}",
        player_left_half.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=PLAYER_RIGHT_HALF_TEMPLATE={}",
        player_right_half.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=PLAYER_TOP_HALF_TEMPLATE={}",
        player_top_half.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=PLAYER_BOTTOM_HALF_TEMPLATE={}",
        player_bottom_half.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=PLAYER_STRANGER_TEMPLATE={}",
        player_stranger.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=PLAYER_GUILDIE_TEMPLATE={}",
        player_guildie.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=PLAYER_FRIEND_TEMPLATE={}",
        player_friend.to_str().unwrap()
    );

    println!(
        "cargo:rustc-env=ESC_MENU_TEMPLATE={}",
        esc_menu.to_str().unwrap()
    );
    println!("cargo:rustc-env=TOMB_TEMPLATE={}", tomb.to_str().unwrap());
    println!(
        "cargo:rustc-env=CASH_SHOP_TEMPLATE={}",
        cash_shop.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ERDA_SHOWER_TEMPLATE={}",
        erda_shower.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=PORTAL_TEMPLATE={}",
        portal.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=CHANGE_CHANNEL_MENU_TEMPLATE={}",
        change_channel_menu.to_str().unwrap()
    );
    println!("cargo:rustc-env=TIMER_TEMPLATE={}", timer.to_str().unwrap());
    println!("cargo:rustc-env=LEVEL_TEMPLATE={}", level.to_str().unwrap());
    println!(
        "cargo:rustc-env=LIE_DETECTOR_SHAPE_TEMPLATE={}",
        lie_detector_shape.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=LIE_DETECTOR_SHAPE_PREPARE_TEMPLATE={}",
        lie_detector_shape_prepare.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=LIE_DETECTOR_VIOLETTA_TEMPLATE={}",
        lie_detector_violetta.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=LIE_DETECTOR_VIOLETTA_FACE_TEMPLATE={}",
        lie_detector_violetta_face.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=LIE_DETECTOR_VIOLETTA_PREPARE_TEMPLATE={}",
        lie_detector_violetta_prepare.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=LIE_DETECTOR_VIOLETTA_NUMBER_TEMPLATE={}",
        lie_detector_violetta_number.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=LIE_DETECTOR_VIOLETTA_NUMBER_MASK_TEMPLATE={}",
        lie_detector_violetta_number_mask.to_str().unwrap()
    );
    println!("cargo:rustc-env=ADMIN_TEMPLATE={}", admin.to_str().unwrap());

    println!("cargo:rustc-env=RUNE_TEMPLATE={}", rune.to_str().unwrap());
    println!(
        "cargo:rustc-env=RUNE_MASK_TEMPLATE={}",
        rune_mask.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=RUNE_BUFF_TEMPLATE={}",
        rune_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=SPIN_TEST_DIR={}",
        spin_test.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=TRANSPARENT_SHAPE_TEST_NORMAL_VIDEO={}",
        transparent_shape_test_normal.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=TRANSPARENT_SHAPE_TEST_HARD_VIDEO={}",
        transparent_shape_test_hard.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=VIOLETTA_TEST_VIDEO={}",
        violetta_test.to_str().unwrap()
    );

    // Collector's buffs
    println!(
        "cargo:rustc-env=SAYRAM_ELIXIR_BUFF_TEMPLATE={}",
        sayram_elixir_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=AURELIA_ELIXIR_BUFF_TEMPLATE={}",
        aurelia_elixir_buff.to_str().unwrap()
    );

    // Exp buffs
    println!(
        "cargo:rustc-env=EXP_COUPON_X2_BUFF_TEMPLATE={}",
        exp_coupon_x2_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=EXP_COUPON_X3_BUFF_TEMPLATE={}",
        exp_coupon_x3_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=EXP_COUPON_X4_BUFF_TEMPLATE={}",
        exp_coupon_x4_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=BONUS_EXP_COUPON_BUFF_TEMPLATE={}",
        bonus_exp_coupon_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=MVP_BONUS_EXP_COUPON_BUFF_TEMPLATE={}",
        mvp_bonus_exp_coupon_buff.to_str().unwrap()
    );

    // Legion buffs
    println!(
        "cargo:rustc-env=LEGION_WEALTH_BUFF_TEMPLATE={}",
        legion_wealth_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=LEGION_WEALTH_BUFF_2_TEMPLATE={}",
        legion_wealth_buff_2.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=LEGION_LUCK_BUFF_TEMPLATE={}",
        legion_luck_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=LEGION_LUCK_BUFF_MASK_TEMPLATE={}",
        legion_luck_buff_mask.to_str().unwrap()
    );

    // Wealth/exp potions
    println!(
        "cargo:rustc-env=WEALTH_ACQUISITION_POTION_BUFF_TEMPLATE={}",
        wealth_acquisition_potion_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=WEALTH_EXP_POTION_MASK_TEMPLATE={}",
        wealth_exp_potion_mask.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=EXP_ACCUMULATION_POTION_BUFF_TEMPLATE={}",
        exp_accumulation_potion_buff.to_str().unwrap()
    );

    // Small wealth/exp potions
    println!(
        "cargo:rustc-env=SMALL_WEALTH_ACQUISITION_POTION_BUFF_TEMPLATE={}",
        small_wealth_acquisition_potion_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=SMALL_WEALTH_EXP_POTION_MASK_TEMPLATE={}",
        small_wealth_exp_potion_mask.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=SMALL_EXP_ACCUMULATION_POTION_BUFF_TEMPLATE={}",
        small_exp_accumulation_potion_buff.to_str().unwrap()
    );

    // Guild buffs
    println!(
        "cargo:rustc-env=FOR_THE_GUILD_BUFF_TEMPLATE={}",
        for_the_guild_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HARD_HITTER_BUFF_TEMPLATE={}",
        hard_hitter_buff.to_str().unwrap()
    );

    // Monster park potions
    println!(
        "cargo:rustc-env=EXTREME_RED_POTION_BUFF_TEMPLATE={}",
        extreme_red_potion_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=EXTREME_BLUE_POTION_BUFF_TEMPLATE={}",
        extreme_blue_potion_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=EXTREME_GREEN_POTION_BUFF_TEMPLATE={}",
        extreme_green_potion_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=EXTREME_GOLD_POTION_BUFF_TEMPLATE={}",
        extreme_gold_potion_buff.to_str().unwrap()
    );

    println!(
        "cargo:rustc-env=HEXA_BOOSTER_TEMPLATE={}",
        hexa_booster.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HEXA_BOOSTER_NUMBER_TEMPLATE={}",
        hexa_booster_number.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HEXA_BOOSTER_NUMBER_MASK_TEMPLATE={}",
        hexa_booster_number_mask.to_str().unwrap()
    );

    println!(
        "cargo:rustc-env=HEXA_MENU_TEMPLATE={}",
        hexa_menu.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HEXA_QUICK_MENU_TEMPLATE={}",
        hexa_quick_menu.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HEXA_BUTTON_ERDA_CONVERSION_TEMPLATE={}",
        hexa_button_erda_conversion.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HEXA_BUTTON_HEXA_BOOSTER_TEMPLATE={}",
        hexa_button_hexa_booster.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HEXA_BUTTON_MAX_TEMPLATE={}",
        hexa_button_max.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HEXA_BUTTON_CONVERT_TEMPLATE={}",
        hexa_button_convert.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HEXA_SOL_ERDA_TEMPLATE={}",
        hexa_sol_erda.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HEXA_SOL_ERDA_FULL_TEMPLATE={}",
        hexa_sol_erda_full.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HEXA_SOL_ERDA_FULL_MASK_TEMPLATE={}",
        hexa_sol_erda_full_mask.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HEXA_SOL_ERDA_EMPTY_TEMPLATE={}",
        hexa_sol_erda_empty.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HEXA_SOL_ERDA_EMPTY_MASK_TEMPLATE={}",
        hexa_sol_erda_empty_mask.to_str().unwrap()
    );

    println!(
        "cargo:rustc-env=HP_BAR_ANCHOR_TEMPLATE={}",
        hp_bar_anchor.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HP_SEPARATOR_TEMPLATE={}",
        hp_separator.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HP_SHIELD_TEMPLATE={}",
        hp_shield.to_str().unwrap()
    );

    println!(
        "cargo:rustc-env=FAMILIAR_BUTTON_SAVE_TEMPLATE={}",
        familiar_button_save.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=FAMILIAR_BUTTON_SETUP_TEMPLATE={}",
        familiar_button_setup.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=FAMILIAR_BUTTON_LEVEL_TEMPLATE={}",
        familiar_button_level.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=FAMILIAR_SLOT_FREE_TEMPLATE={}",
        familiar_slot_free.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=FAMILIAR_SLOT_OCCUPIED_TEMPLATE={}",
        familiar_slot_occupied.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=FAMILIAR_SLOT_OCCUPIED_MASK_TEMPLATE={}",
        familiar_slot_occupied_mask.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=FAMILIAR_LEVEL_5_TEMPLATE={}",
        familiar_level_5.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=FAMILIAR_LEVEL_5_MASK_TEMPLATE={}",
        familiar_level_5_mask.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=FAMILIAR_SCROLLBAR_TEMPLATE={}",
        familiar_scrollbar.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=FAMILIAR_CARD_RARE_TEMPLATE={}",
        familiar_card_rare.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=FAMILIAR_CARD_EPIC_TEMPLATE={}",
        familiar_card_epic.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=FAMILIAR_CARD_MASK_TEMPLATE={}",
        familiar_card_mask.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=FAMILIAR_BUFF_TEMPLATE={}",
        familiar_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=FAMILIAR_MENU_TEMPLATE={}",
        familiar_menu.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=FAMILIAR_ESSENCE_DEPLETE_TEMPLATE={}",
        familiar_essence_deplete.to_str().unwrap()
    );

    // onnxruntime dependencies
    let profile = if let Ok(profile) = env::var("PROFILE") {
        profile.leak()
    } else if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let target = if let Ok(target) = env::var("TARGET") {
        target.leak()
    } else if cfg!(all(
        target_arch = "x86_64",
        target_os = "windows",
        target_env = "msvc"
    )) {
        "x86_64-pc-windows-msvc"
    } else {
        panic!("unsupported target")
    };
    let dx_exe_dir = target_dir
        .join("dx")
        .join("ui")
        .join(profile)
        .join("windows")
        .join("app");
    let normal_exe_dir = target_dir.join(target).join(profile);
    let _ = fs::create_dir_all(&normal_exe_dir);
    let _ = fs::create_dir_all(&dx_exe_dir);

    p!("dioxus directory: {}", dx_exe_dir.to_str().unwrap());
    p!("normal directory: {}", normal_exe_dir.to_str().unwrap());

    copy_file_to_dir(&onnx_runtime, &dx_exe_dir);
    copy_file_to_dir(&onnx_runtime, &normal_exe_dir);
    copy_file_to_dir(&onnx_runtime_shared, &dx_exe_dir);
    copy_file_to_dir(&onnx_runtime_shared, &normal_exe_dir);

    let tools_dir = dir.parent().unwrap().join("tools");
    let join_script = tools_dir.join("join.ps1").to_str().unwrap().to_string();

    let _ = Command::new("powershell")
        .arg("-Command")
        .arg(format!(
            "& {{ . {}; join {} {}}}",
            &join_script,
            onnx_runtime_cuda.to_str().unwrap(),
            dx_exe_dir
                .join(onnx_runtime_cuda.file_name().unwrap())
                .to_str()
                .unwrap()
        ))
        .spawn()
        .expect("failed to spawn powershell command")
        .wait();
    println!(
        "cargo:rerun-if-changed={}",
        dx_exe_dir
            .join(onnx_runtime_cuda.file_name().unwrap())
            .to_str()
            .unwrap()
    );

    let _ = Command::new("powershell")
        .arg("-Command")
        .arg(format!(
            "& {{ . {}; join {} {}}}",
            &join_script,
            onnx_runtime_cuda.to_str().unwrap(),
            normal_exe_dir
                .join(onnx_runtime_cuda.file_name().unwrap())
                .to_str()
                .unwrap()
        ))
        .spawn()
        .expect("failed to spawn powershell command")
        .wait();
    println!(
        "cargo:rerun-if-changed={}",
        normal_exe_dir
            .join(onnx_runtime_cuda.file_name().unwrap())
            .to_str()
            .unwrap()
    );

    // Detection models
    println!("cargo:rustc-env=MOB_MODEL={}", mob_model.to_str().unwrap());
    println!(
        "cargo:rustc-env=MINIMAP_MODEL={}",
        minimap_model.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=TRANSPARENT_SHAPE_MODEL={}",
        transparent_shape_model.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=VIOLETTA_MODEL={}",
        violetta_model.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=RUNE_MODEL={}",
        rune_model.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=RUNE_SPIN_MODEL={}",
        rune_spin_model.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=TEXT_DETECTION_MODEL={}",
        text_detection_model.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=TEXT_RECOGNITION_MODEL={}",
        text_recognition_model.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=TEXT_RECOGNITION_ALPHABET={}",
        text_alphabet_txt.to_str().unwrap()
    );
}

fn copy_file_to_dir(file: &PathBuf, dir: &Path) {
    let destination = dir.join(file.file_name().unwrap());
    let destination_str = destination.to_str().unwrap().to_string();
    let _ = fs::copy(file, destination);
    println!("cargo:rerun-if-changed={}", destination_str);
}
