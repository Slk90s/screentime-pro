//! pet/state.rs — 桌宠状态 SQLite 持久化与 localStorage 幂等迁移。
//! 修改历史：2026-11-03 @v1.2.12: 新增 - 单键 JSON 读写及备份导出/导入支持。

use crate::AppState;
use std::sync::Arc;
use tauri::State;

const PET_STATE_KEY: &str = "pet_state";

fn validate_json(json: &str) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("桌宠状态 JSON 无效: {e}"))?;
    if value.is_object() {
        Ok(())
    } else {
        Err("桌宠状态必须是 JSON 对象".to_string())
    }
}

#[tauri::command]
pub fn load_pet_state(
    state: State<'_, Arc<AppState>>,
    legacy_json: String,
) -> Result<String, String> {
    if let Some(json) = state
        .db
        .get_setting_result(PET_STATE_KEY)
        .map_err(|e| e.to_string())?
    {
        return Ok(json);
    }
    validate_json(&legacy_json)?;
    state
        .db
        .set_setting_if_absent(PET_STATE_KEY, &legacy_json)
        .map_err(|e| e.to_string())?;
    state
        .db
        .get_setting_result(PET_STATE_KEY)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "桌宠状态迁移后读取失败".to_string())
}

#[tauri::command]
pub fn save_pet_state(state: State<'_, Arc<AppState>>, json: String) -> Result<bool, String> {
    validate_json(&json)?;
    state
        .db
        .set_setting(PET_STATE_KEY, &json)
        .map_err(|e| e.to_string())?;
    Ok(true)
}
