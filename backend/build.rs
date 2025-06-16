use std::{env, fs, path::PathBuf};

fn main() {
    let dir = env::current_dir().unwrap().join("resources");
    let esc_setting = dir.join("esc_setting_ideal_ratio.png");
    let esc_menu = dir.join("esc_menu_ideal_ratio.png");
    let esc_event = dir.join("esc_event_ideal_ratio.png");
    let esc_community = dir.join("esc_community_ideal_ratio.png");
    let esc_character = dir.join("esc_character_ideal_ratio.png");
    let esc_ok = dir.join("esc_ok_ideal_ratio.png");
    let esc_cancel = dir.join("esc_ok_ideal_ratio.png");
    let esc_end_chat = dir.join("esc_end_chat_ideal_ratio.png");
    let esc_next = dir.join("esc_next_ideal_ratio.png");
    let tomb = dir.join("tomb_ideal_ratio.png");
    let tomb_ok = dir.join("tomb_button_ok_ideal_ratio.png");
    let elite_boss_bar_1 = dir.join("elite_boss_bar_1_ideal_ratio.png");
    let elite_boss_bar_2 = dir.join("elite_boss_bar_2_ideal_ratio.png");
    let player = dir.join("player_ideal_ratio.png");
    let player_stranger = dir.join("player_stranger_ideal_ratio.png");
    let player_guildie = dir.join("player_guildie_ideal_ratio.png");
    let player_friend = dir.join("player_friend_ideal_ratio.png");
    let erda_shower = dir.join("erda_shower_ideal_ratio.png");
    let portal = dir.join("portal_ideal_ratio.png");
    let rune = dir.join("rune_ideal_ratio.png");
    let rune_mask = dir.join("rune_mask_ideal_ratio.png");
    let rune_buff = dir.join("rune_buff_ideal_ratio.png");
    let sayram_elixir_buff = dir.join("sayram_elixir_buff_ideal_ratio.png");
    let aurelia_elixir_buff = dir.join("aurelia_elixir_buff_ideal_ratio.png");
    let exp_coupon_x3_buff = dir.join("exp_coupon_x3_buff_ideal_ratio.png");
    let bonus_exp_coupon_buff = dir.join("bonus_exp_coupon_buff_ideal_ratio.png");
    let legion_wealth_buff = dir.join("legion_wealth_buff_ideal_ratio.png");
    let legion_wealth_buff_2 = dir.join("legion_wealth_buff_2_ideal_ratio.png");
    let legion_luck_buff = dir.join("legion_luck_buff_ideal_ratio.png");
    let legion_luck_buff_mask = dir.join("legion_luck_buff_mask_ideal_ratio.png");
    let wealth_acquisition_potion_buff = dir.join("wealth_acquisition_potion_ideal_ratio.png");
    let wealth_exp_potion_mask = dir.join("wealth_exp_potion_mask_ideal_ratio.png");
    let exp_accumulation_potion_buff = dir.join("exp_accumulation_potion_ideal_ratio.png");
    let extreme_red_potion_buff = dir.join("extreme_red_potion_ideal_ratio.png");
    let extreme_blue_potion_buff = dir.join("extreme_blue_potion_ideal_ratio.png");
    let extreme_green_potion_buff = dir.join("extreme_green_potion_ideal_ratio.png");
    let extreme_gold_potion_buff = dir.join("extreme_gold_potion_ideal_ratio.png");
    let cash_shop = dir.join("cash_shop.png");
    let hp_start = dir.join("hp_start_ideal_ratio.png");
    let hp_separator_1 = dir.join("hp_separator_ideal_ratio_1.png");
    let hp_separator_2 = dir.join("hp_separator_ideal_ratio_2.png");
    let hp_shield = dir.join("hp_shield_ideal_ratio.png");
    let hp_end = dir.join("hp_end_ideal_ratio.png");
    let spin_test = dir.join("spin_test_2");
    let familiar_button_save = dir.join("familiar_button_save_ideal_ratio.png");
    let familiar_button_setup = dir.join("familiar_button_setup_ideal_ratio.png");
    let familiar_slot_free = dir.join("familiar_slot_free_ideal_ratio.png");
    let familiar_slot_occupied = dir.join("familiar_slot_occupied_ideal_ratio.png");
    let familiar_slot_occupied_mask = dir.join("familiar_slot_occupied_mask_ideal_ratio.png");
    let familiar_level_5 = dir.join("familiar_level_5_ideal_ratio.png");
    let familiar_level_5_mask = dir.join("familiar_level_5_mask_ideal_ratio.png");
    let familiar_scrollbar = dir.join("familiar_scrollbar_ideal_ratio.png");
    let familiar_card_rare = dir.join("familiar_card_rare_ideal_ratio.png");
    let familiar_card_epic = dir.join("familiar_card_epic_ideal_ratio.png");
    let familiar_card_mask = dir.join("familiar_card_mask_ideal_ratio.png");
    let familiar_buff = dir.join("familiar_buff_ideal_ratio.png");
    let familiar_menu = dir.join("familiar_menu_ideal_ratio.png");
    let familiar_essence_deplete = dir.join("familiar_essence_deplete_ideal_ratio.png");
    let maple_guide_menu = dir.join("maple_guide_menu_ideal_ratio.png");
    let maple_guide_town = dir.join("maple_guide_town_ideal_ratio.png");
    let maple_guide_town_mask = dir.join("maple_guide_town_mask_ideal_ratio.png");
    let change_channel_menu = dir.join("change_channel_menu_ideal_ratio.png");

    let mob_model = dir.join("mob_nms.onnx");
    let rune_model = dir.join("rune_nms.onnx");
    let minimap_model = dir.join("minimap_nms.onnx");
    let onnx_runtime = dir.join("onnxruntime/onnxruntime.dll");
    #[cfg(feature = "gpu")]
    let onnx_runtime_cuda = dir.join("onnxruntime/onnxruntime_providers_cuda.dll");
    #[cfg(feature = "gpu")]
    let onnx_runtime_shared = dir.join("onnxruntime/onnxruntime_providers_shared.dll");
    let text_detection_model = dir.join("text_detection.onnx");
    let text_recognition_model = dir.join("text_recognition.onnx");
    let text_alphabet_txt = dir.join("alphabet_94.txt");

    tonic_build::compile_protos("proto/input.proto").unwrap();
    println!(
        "cargo:rustc-env=ESC_SETTING_TEMPLATE={}",
        esc_setting.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ESC_MENU_TEMPLATE={}",
        esc_menu.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ESC_EVENT_TEMPLATE={}",
        esc_event.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ESC_COMMUNITY_TEMPLATE={}",
        esc_community.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ESC_CHARACTER_TEMPLATE={}",
        esc_character.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ESC_OK_TEMPLATE={}",
        esc_ok.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ESC_CANCEL_TEMPLATE={}",
        esc_cancel.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ESC_END_CHAT_TEMPLATE={}",
        esc_end_chat.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ESC_NEXT_TEMPLATE={}",
        esc_next.to_str().unwrap()
    );
    println!("cargo:rustc-env=TOMB_TEMPLATE={}", tomb.to_str().unwrap());
    println!(
        "cargo:rustc-env=TOMB_BUTTON_OK_TEMPLATE={}",
        tomb_ok.to_str().unwrap()
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
        "cargo:rustc-env=ERDA_SHOWER_TEMPLATE={}",
        erda_shower.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=PORTAL_TEMPLATE={}",
        portal.to_str().unwrap()
    );
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
        "cargo:rustc-env=SAYRAM_ELIXIR_BUFF_TEMPLATE={}",
        sayram_elixir_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=AURELIA_ELIXIR_BUFF_TEMPLATE={}",
        aurelia_elixir_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=EXP_COUPON_X3_BUFF_TEMPLATE={}",
        exp_coupon_x3_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=BONUS_EXP_COUPON_BUFF_TEMPLATE={}",
        bonus_exp_coupon_buff.to_str().unwrap()
    );
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
        "cargo:rustc-env=CASH_SHOP_TEMPLATE={}",
        cash_shop.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HP_START_TEMPLATE={}",
        hp_start.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HP_SEPARATOR_1_TEMPLATE={}",
        hp_separator_1.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HP_SEPARATOR_2_TEMPLATE={}",
        hp_separator_2.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HP_SHIELD_TEMPLATE={}",
        hp_shield.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HP_END_TEMPLATE={}",
        hp_end.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=SPIN_TEST_DIR={}",
        spin_test.to_str().unwrap()
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
    println!(
        "cargo:rustc-env=MAPLE_GUIDE_MENU_TEMPLATE={}",
        maple_guide_menu.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=MAPLE_GUIDE_TOWN_TEMPLATE={}",
        maple_guide_town.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=MAPLE_GUIDE_TOWN_MASK_TEMPLATE={}",
        maple_guide_town_mask.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=CHANGE_CHANNEL_MENU_TEMPLATE={}",
        change_channel_menu.to_str().unwrap()
    );

    // onnxruntime dependencies
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = out_dir.ancestors().nth(5).unwrap();
    let exe_build_type = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let exe_dir = target_dir
        .join("dx")
        .join("ui")
        .join(exe_build_type)
        .join("windows")
        .join("app");
    let _ = fs::create_dir_all(&exe_dir);

    fs::copy(
        &onnx_runtime,
        exe_dir.join(onnx_runtime.file_name().unwrap()),
    )
    .unwrap();
    #[cfg(feature = "gpu")]
    fs::copy(
        &onnx_runtime_cuda,
        exe_dir.join(onnx_runtime_cuda.file_name().unwrap()),
    )
    .unwrap();
    #[cfg(feature = "gpu")]
    fs::copy(
        &onnx_runtime_shared,
        exe_dir.join(onnx_runtime_shared.file_name().unwrap()),
    )
    .unwrap();
    println!("cargo:rerun-if-changed={}", exe_dir.to_str().unwrap());

    // Detection models
    println!("cargo:rustc-env=MOB_MODEL={}", mob_model.to_str().unwrap());
    println!(
        "cargo:rustc-env=MINIMAP_MODEL={}",
        minimap_model.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=RUNE_MODEL={}",
        rune_model.to_str().unwrap()
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
