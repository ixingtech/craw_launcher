mod build_locale;
mod cli;

use chrono::Utc;
use reqwest::blocking::Client;
use rfd::FileDialog;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::{
    collections::HashSet,
    env,
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, SystemTime},
};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;
use walkdir::WalkDir;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

fn is_english_build() -> bool {
    build_locale::BUILD_LOCALE == "en-US"
}

#[derive(Default)]
struct RuntimeState {
    gateway: Mutex<GatewayRuntime>,
    subscriber: Mutex<SubscriberRuntime>,
}

#[derive(Clone, Debug)]
struct LauncherContext {
    app_data_dir: PathBuf,
}

#[derive(Default)]
struct GatewayRuntime {
    child: Option<Child>,
    status: GatewayStatus,
    probe_in_flight: bool,
    last_probe_at: Option<SystemTime>,
}

#[derive(Default)]
struct SubscriberRuntime {
    child: Option<Child>,
    profile_id: Option<String>,
    gateway_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ValidationInput {
    executable_path: Option<String>,
    data_dir: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ValidationResult {
    executable_path: Option<String>,
    install_dir: Option<String>,
    inferred_data_dir: Option<String>,
    supports_profile_switch: bool,
    is_valid: bool,
    issues: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PathCandidate {
    executable_path: String,
    data_dir: Option<String>,
    source: String,
    score: i32,
    validation: ValidationResult,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LaunchRecord {
    profile_id: String,
    profile_name: String,
    launched_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GatewayConfig {
    mode: String,
    command: Option<String>,
    args: Vec<String>,
    url: String,
    health_endpoint: String,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            mode: "manual".into(),
            command: None,
            args: Vec::new(),
            url: "http://127.0.0.1:3000".into(),
            health_endpoint: "/health".into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    openclaw_executable_path: Option<String>,
    openclaw_data_dir: Option<String>,
    profiles_root: Option<String>,
    gateway_config: GatewayConfig,
    recent_profile_id: Option<String>,
    recent_launches: Vec<LaunchRecord>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            openclaw_executable_path: None,
            openclaw_data_dir: None,
            profiles_root: None,
            gateway_config: GatewayConfig::default(),
            recent_profile_id: None,
            recent_launches: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PackageMeta {
    package_name: String,
    zip_path: String,
    source_dir: String,
    exported_at: String,
    file_count: usize,
    include_memory: bool,
    include_account_info: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ExportProfileRequest {
    source_dir: String,
    zip_path: Option<String>,
    package_name: Option<String>,
    include_memory: Option<bool>,
    include_account_info: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProfileMeta {
    id: String,
    name: String,
    imported_from: Option<String>,
    created_at: String,
    last_used_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ManagedProfile {
    id: String,
    name: String,
    path: String,
    imported_from: Option<String>,
    created_at: String,
    last_used_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProfileListItem {
    id: String,
    title: String,
    subtitle: String,
    updated_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProfileItemPreview {
    title: String,
    subtitle: String,
    content: String,
    updated_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct ProfileInventory {
    setting_documents: Vec<ProfileListItem>,
    skills: Vec<ProfileListItem>,
    cron_jobs: Vec<ProfileListItem>,
    memories: Vec<ProfileListItem>,
    accounts: Vec<ProfileListItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NotificationItem {
    id: String,
    title: String,
    subtitle: String,
    content: String,
    created_at: String,
    kind: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LaunchHandle {
    pid: Option<u32>,
    started_at: String,
    profile_id: String,
    profile_name: String,
    executable_path: String,
    args: Vec<String>,
    connection_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct GatewayStatus {
    mode: String,
    url: String,
    running: bool,
    pid: Option<u32>,
    started_at: Option<String>,
    healthy: bool,
    last_error: Option<String>,
    log_tail: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChatMessage {
    id: String,
    role: String,
    content: String,
    created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Conversation {
    id: String,
    title: String,
    created_at: String,
    updated_at: String,
    messages: Vec<ChatMessage>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ConversationSummary {
    id: String,
    title: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChatRequest {
    conversation_id: String,
    content: String,
    profile_id: Option<String>,
    model: Option<String>,
    params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChatDeltaEvent {
    conversation_id: String,
    content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChatDoneEvent {
    conversation_id: String,
    conversation: Conversation,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChatErrorEvent {
    conversation_id: String,
    error: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChatConversationEvent {
    conversation_id: String,
    conversation: Conversation,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcpStreamPayload {
    openclaw_path: String,
    cwd: String,
    profile_name: Option<String>,
    session_key: String,
    gateway_url: Option<String>,
    gateway_token: Option<String>,
    gateway_password: Option<String>,
    sdk_path: String,
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum AcpStreamEvent {
    Delta {
        content: String,
    },
    Done {
        #[serde(rename = "stopReason")]
        stop_reason: String,
    },
    Error {
        error: String,
    },
    Tool {
        title: Option<String>,
        status: Option<String>,
    },
    #[serde(rename = "tool-update")]
    ToolUpdate {
        #[serde(rename = "toolCallId")]
        tool_call_id: Option<String>,
        status: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GatewaySubscriberPayload {
    gateway_url: String,
    gateway_token: Option<String>,
    gateway_password: Option<String>,
    ws_module_path: String,
    client_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum GatewaySubscriberEvent {
    Status {
        status: String,
    },
    Message {
        #[serde(rename = "sessionKey")]
        session_key: String,
        text: String,
        timestamp: String,
    },
    Error {
        error: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageManifest {
    format_version: u32,
    package_name: String,
    exported_at: String,
    source_dir_name: String,
    version: Option<String>,
    include_memory: bool,
    include_account_info: bool,
    entries: Vec<ManifestEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ManifestEntry {
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ImportVerification {
    valid: bool,
    package_name: Option<String>,
    exported_at: Option<String>,
    issues: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OpenClawFileConfig {
    gateway: Option<OpenClawFileGateway>,
}

#[derive(Debug, Deserialize)]
struct OpenClawFileGateway {
    port: Option<u64>,
    auth: Option<OpenClawFileGatewayAuth>,
    remote: Option<OpenClawFileGatewayRemote>,
}

#[derive(Debug, Deserialize)]
struct OpenClawFileGatewayAuth {
    mode: Option<String>,
    token: Option<String>,
    password: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenClawFileGatewayRemote {
    url: Option<String>,
}

const SETTINGS_FILE: &str = "settings.json";
const CONVERSATIONS_DIR: &str = "conversations";
const PROFILE_META_FILE: &str = ".openclaw-profile.json";
const MANIFEST_FILE: &str = "manifest.json";
const LOCAL_PROFILE_ID: &str = "__local__";
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[tauri::command]
#[allow(unreachable_code)]
fn pick_openclaw_executable() -> Option<String> {
    pick_openclaw_executable_impl()
}

#[tauri::command]
#[allow(unreachable_code)]
fn pick_directory() -> Option<String> {
    pick_directory_impl()
}

#[tauri::command]
fn pick_zip_file() -> Option<String> {
    FileDialog::new()
        .add_filter("Claw package", &["claw"])
        .pick_file()
        .map(|path| path.display().to_string())
}

#[tauri::command]
fn pick_save_zip_path(default_name: Option<String>) -> Option<String> {
    let mut dialog = FileDialog::new().add_filter("Claw package", &["claw"]);
    if let Some(default_name) = default_name {
        dialog = dialog.set_file_name(&default_name);
    }
    dialog.save_file().map(|path| path.display().to_string())
}

#[tauri::command]
fn desktop_path() -> Option<String> {
    default_desktop_path().map(|path| path.display().to_string())
}

pub fn run() {
    tauri::Builder::default()
        .manage(RuntimeState::default())
        .setup(|app| {
            if let Ok(settings) = load_settings(app.handle().clone()) {
                sync_runtime_gateway_defaults(app.handle(), &settings);
            }
            let _ = process_launch_file_args(app.handle().clone());
            start_gateway_status_poller(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            detect_openclaw,
            validate_openclaw_path,
            load_settings,
            save_settings,
            list_profiles,
            inspect_profile,
            list_notifications,
            preview_profile_item,
            read_profile_readme,
            save_profile_readme,
            rename_profile,
            export_profile,
            verify_import_package,
            import_profile,
            delete_profile,
            launch_openclaw,
            open_control_web,
            start_gateway,
            stop_gateway,
            gateway_status,
            list_conversation_summaries,
            get_conversation,
            list_conversations,
            save_conversation,
            delete_conversation,
            send_chat_message,
            pick_openclaw_executable,
            pick_directory,
            desktop_path,
            pick_zip_file,
            pick_save_zip_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenClaw Launcher");
}

pub fn cli_main() -> Result<(), String> {
    cli::run()
}

#[tauri::command]
fn detect_openclaw(app: AppHandle) -> Result<Vec<PathCandidate>, String> {
    let settings = load_settings(app.clone())?;
    let mut candidates: Vec<(String, i32)> = Vec::new();
    if let Some(saved) = settings.openclaw_executable_path.clone() {
        candidates.push((saved, 120));
    }
    candidates.extend(common_openclaw_candidates());

    let mut output = Vec::new();
    let mut seen = HashSet::new();
    for (path, score) in candidates {
        if !seen.insert(path.clone()) {
            continue;
        }
        let validation = validate_paths(
            Some(path.clone()),
            settings.openclaw_data_dir.clone().or_else(|| {
                default_openclaw_data_dir_path().map(|value| value.display().to_string())
            }),
        );
        if validation.executable_path.is_none() {
            continue;
        }
        output.push(PathCandidate {
            executable_path: validation.executable_path.clone().unwrap_or(path),
            data_dir: validation.inferred_data_dir.clone(),
            source: if score >= 120 { "saved" } else { "scan" }.into(),
            score,
            validation,
        });
    }
    output.sort_by(|left, right| right.score.cmp(&left.score));
    Ok(output)
}

#[tauri::command]
fn validate_openclaw_path(input: ValidationInput) -> Result<ValidationResult, String> {
    Ok(validate_paths(input.executable_path, input.data_dir))
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Result<AppSettings, String> {
    let path = settings_path(&app)?;
    let settings_file_exists = path.exists();
    let mut settings = if path.exists() {
        read_json::<AppSettings>(&path)?
    } else {
        AppSettings::default()
    };

    let mut normalized = false;

    if let Some(value) = settings.openclaw_executable_path.clone() {
        if let Some(resolved) = normalize_openclaw_command_path(Path::new(&value)) {
            let resolved_display = resolved.display().to_string();
            if resolved_display != value {
                settings.openclaw_executable_path = Some(resolved_display);
                normalized = true;
            }
        }
    }

    if settings
        .openclaw_executable_path
        .as_ref()
        .is_some_and(|value| !is_valid_openclaw_command_path(Path::new(value)))
    {
        settings.openclaw_executable_path = None;
    }

    if settings
        .openclaw_data_dir
        .as_ref()
        .is_some_and(|value| !looks_like_openclaw_data_dir(Path::new(value)))
    {
        settings.openclaw_data_dir = None;
    }

    if settings.openclaw_data_dir.is_none() {
        settings.openclaw_data_dir =
            default_openclaw_data_dir_path().map(|path| path.display().to_string());
    }

    if settings.openclaw_executable_path.is_none() {
        settings.openclaw_executable_path =
            detect_first_openclaw_command().map(|path| path.display().to_string());
        normalized = normalized || settings.openclaw_executable_path.is_some();
    }

    if settings.profiles_root.is_none() || settings_uses_legacy_profiles_root(&app, &settings) {
        settings.profiles_root = Some(default_profiles_root(&app)?.display().to_string());
    }

    migrate_legacy_profiles_if_needed(&app, &settings)?;

    apply_gateway_defaults(&mut settings);
    if normalized || !settings_file_exists {
        write_json(&path, &settings)?;
    }
    Ok(settings)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    let path = settings_path(&app)?;
    let mut normalized_settings = settings;
    normalized_settings.openclaw_executable_path = normalized_settings
        .openclaw_executable_path
        .as_ref()
        .and_then(|value| {
            normalize_openclaw_command_path(Path::new(value)).map(|path| display(&path))
        });
    write_json(&path, &normalized_settings)?;
    sync_runtime_gateway_defaults(&app, &normalized_settings);
    Ok(normalized_settings)
}

#[tauri::command]
#[allow(unreachable_code)]
fn list_profiles(app: AppHandle) -> Result<Vec<ManagedProfile>, String> {
    list_profiles_impl(app)
}

#[tauri::command]
fn export_profile(request: ExportProfileRequest) -> Result<PackageMeta, String> {
    export_profile_impl(request)
}

#[tauri::command]
fn verify_import_package(zip_path: String) -> Result<ImportVerification, String> {
    if !has_claw_extension(Path::new(&zip_path)) {
        return Err("只能导入 .claw 格式的龙虾包。".to_string());
    }
    verify_import_package_impl(Path::new(&zip_path))
}

#[tauri::command]
fn import_profile(
    app: AppHandle,
    zip_path: String,
    requested_name: Option<String>,
    ignore_verification: Option<bool>,
) -> Result<ManagedProfile, String> {
    import_profile_impl(
        &app,
        PathBuf::from(zip_path),
        requested_name,
        ignore_verification.unwrap_or(false),
    )
}

fn import_profile_impl(
    app: &AppHandle,
    zip_path: PathBuf,
    requested_name: Option<String>,
    ignore_verification: bool,
) -> Result<ManagedProfile, String> {
    if !zip_path.is_file() {
        return Err("没有找到要导入的压缩包.".into());
    }
    if !has_claw_extension(&zip_path) {
        return Err("只能导入 .claw 格式的龙虾包。".into());
    }

    let verification = verify_import_package_impl(&zip_path)?;
    if !verification.valid && !ignore_verification {
        return Err("你正尝试导入的龙虾被篡改过.".into());
    }

    let mut settings = load_settings(app.clone())?;
    let root = profiles_root(app, &settings)?;
    fs::create_dir_all(&root).map_err(to_string_error)?;
    let reader = File::open(&zip_path).map_err(to_string_error)?;
    let mut archive = ZipArchive::new(reader).map_err(to_string_error)?;
    let manifest = read_package_manifest(&mut archive)?;

    let base_name = requested_name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            zip_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
    let display_name = sanitize_name(&base_name);
    let profile_id = Uuid::new_v4().to_string();
    let cli_name = cli_profile_name_for(&display_name, &profile_id);
    let final_dir_name = unique_profile_dir_name(&root, &cli_name);
    let target = root.join(&final_dir_name);
    let temp_target = root.join(format!("{final_dir_name}.importing"));
    if temp_target.exists() {
        fs::remove_dir_all(&temp_target).map_err(to_string_error)?;
    }
    fs::create_dir_all(&temp_target).map_err(to_string_error)?;

    let extraction_result = (|| -> Result<(), String> {
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(to_string_error)?;
            let Some(enclosed) = entry.enclosed_name().map(PathBuf::from) else {
                continue;
            };
            let destination = temp_target.join(enclosed);
            if entry.is_dir() {
                fs::create_dir_all(&destination).map_err(to_string_error)?;
            } else {
                ensure_parent(&destination)?;
                let mut output = File::create(&destination).map_err(to_string_error)?;
                std::io::copy(&mut entry, &mut output).map_err(to_string_error)?;
            }
        }
        Ok(())
    })();

    if let Err(error) = extraction_result {
        let _ = fs::remove_dir_all(&temp_target);
        return Err(error);
    }

    let profile = ManagedProfile {
        id: profile_id,
        name: display_name.clone(),
        path: target.display().to_string(),
        imported_from: Some(zip_path.display().to_string()),
        created_at: manifest.exported_at,
        last_used_at: None,
    };
    write_json(
        &temp_target.join(PROFILE_META_FILE),
        &ProfileMeta {
            id: profile.id.clone(),
            name: profile.name.clone(),
            imported_from: profile.imported_from.clone(),
            created_at: profile.created_at.clone(),
            last_used_at: profile.last_used_at.clone(),
        },
    )?;
    normalize_managed_profile_runtime(&temp_target, &profile.id)?;
    fs::rename(&temp_target, &target).map_err(to_string_error)?;

    settings.recent_profile_id = Some(profile.id.clone());
    save_settings(app.clone(), settings)?;
    Ok(profile)
}

#[tauri::command]
fn inspect_profile(app: AppHandle, profile_id: String) -> Result<ProfileInventory, String> {
    let settings = load_settings(app.clone())?;
    let root = resolve_profile_root(&app, &settings, &profile_id)?;
    profile_inventory(&root)
}

#[tauri::command]
fn list_notifications(app: AppHandle, profile_id: String) -> Result<Vec<NotificationItem>, String> {
    let settings = load_settings(app.clone())?;
    let root = resolve_profile_root(&app, &settings, &profile_id)?;
    collect_notifications(&root)
}

#[tauri::command]
fn preview_profile_item(
    app: AppHandle,
    profile_id: String,
    section: String,
    item_id: String,
) -> Result<ProfileItemPreview, String> {
    let settings = load_settings(app.clone())?;
    let root = resolve_profile_root(&app, &settings, &profile_id)?;
    preview_profile_item_impl(&root, &section, &item_id)
}

#[tauri::command]
fn read_profile_readme(
    app: AppHandle,
    profile_id: String,
) -> Result<Option<ProfileItemPreview>, String> {
    let settings = load_settings(app.clone())?;
    let root = resolve_profile_root(&app, &settings, &profile_id)?;
    read_profile_readme_impl(&root)
}

#[tauri::command]
fn save_profile_readme(
    app: AppHandle,
    profile_id: String,
    content: String,
) -> Result<ProfileItemPreview, String> {
    let settings = load_settings(app.clone())?;
    let root = resolve_profile_root(&app, &settings, &profile_id)?;
    save_profile_readme_impl(&root, &content)
}

#[tauri::command]
fn delete_profile(app: AppHandle, profile_id: String) -> Result<(), String> {
    if profile_id.is_empty() || profile_id == LOCAL_PROFILE_ID {
        return Err("默认本地龙虾不能删除.".into());
    }

    let mut settings = load_settings(app.clone())?;
    let profile = list_profiles_impl(app.clone())?
        .into_iter()
        .find(|item| item.id == profile_id)
        .ok_or_else(|| "没有找到要删除的龙虾.".to_string())?;
    let path = PathBuf::from(profile.path);
    if path.exists() {
        fs::remove_dir_all(path).map_err(to_string_error)?;
    }

    remove_profile_conversations(&app, &profile_id)?;
    settings
        .recent_launches
        .retain(|item| item.profile_id != profile_id);
    if settings.recent_profile_id.as_deref() == Some(profile_id.as_str()) {
        settings.recent_profile_id = Some(LOCAL_PROFILE_ID.to_string());
    }
    save_settings(app, settings)?;
    Ok(())
}

#[tauri::command]
fn rename_profile(
    app: AppHandle,
    profile_id: String,
    name: String,
) -> Result<ManagedProfile, String> {
    if profile_id.is_empty() || profile_id == LOCAL_PROFILE_ID {
        return Err("默认本地龙虾不能改名.".into());
    }

    let next_name = name.trim();
    if next_name.is_empty() {
        return Err("龙虾名称不能为空.".into());
    }

    let mut settings = load_settings(app.clone())?;
    let profile = list_profiles_impl(app.clone())?
        .into_iter()
        .find(|item| item.id == profile_id)
        .ok_or_else(|| "没有找到要改名的龙虾.".to_string())?;
    let path = PathBuf::from(&profile.path);
    if !path.is_dir() {
        return Err("这只龙虾的目录不存在，无法改名。".into());
    }

    let updated = ManagedProfile {
        name: next_name.to_string(),
        ..profile
    };
    write_json(
        &path.join(PROFILE_META_FILE),
        &ProfileMeta {
            id: updated.id.clone(),
            name: updated.name.clone(),
            imported_from: updated.imported_from.clone(),
            created_at: updated.created_at.clone(),
            last_used_at: updated.last_used_at.clone(),
        },
    )?;

    if settings.recent_profile_id.as_deref() == Some(&updated.id) {
        settings.recent_profile_id = Some(updated.id.clone());
    }
    for launch in &mut settings.recent_launches {
        if launch.profile_id == updated.id {
            launch.profile_name = updated.name.clone();
        }
    }
    save_settings(app, settings)?;
    Ok(updated)
}

#[tauri::command]
#[allow(unreachable_code)]
fn launch_openclaw(
    app: AppHandle,
    profile_id: String,
    state: State<'_, RuntimeState>,
) -> Result<LaunchHandle, String> {
    launch_openclaw_impl(app, profile_id, state)
}

#[tauri::command]
#[allow(unreachable_code)]
fn open_control_web(
    app: AppHandle,
    profile_id: String,
    state: State<'_, RuntimeState>,
) -> Result<(), String> {
    open_control_web_impl(app, profile_id, state)
}

#[tauri::command]
#[allow(unreachable_code)]
fn start_gateway(
    mode: String,
    gateway_config: GatewayConfig,
    state: State<'_, RuntimeState>,
) -> Result<GatewayStatus, String> {
    start_gateway_impl(mode, gateway_config, state)
}

#[tauri::command]
#[allow(unreachable_code)]
fn stop_gateway(state: State<'_, RuntimeState>) -> Result<GatewayStatus, String> {
    stop_gateway_impl(state)
}

#[tauri::command]
#[allow(unreachable_code)]
fn gateway_status(app: AppHandle, state: State<'_, RuntimeState>) -> Result<GatewayStatus, String> {
    gateway_status_impl(app, state)
}

#[tauri::command]
fn list_conversation_summaries(app: AppHandle) -> Result<Vec<ConversationSummary>, String> {
    let root = conversations_root(&app)?;
    fs::create_dir_all(&root).map_err(to_string_error)?;
    let mut conversations = Vec::new();
    for entry in fs::read_dir(root).map_err(to_string_error)? {
        let entry = entry.map_err(to_string_error)?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        if let Some(conversation) = read_or_build_conversation_summary(&app, &path)? {
            conversations.push(conversation);
        }
    }
    conversations.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(conversations)
}

#[tauri::command]
fn get_conversation(app: AppHandle, conversation_id: String) -> Result<Conversation, String> {
    let path = conversation_path(&app, &conversation_id)?;
    if !path.exists() {
        return Err("没有找到这条对话。".into());
    }
    let mut conversation = read_json::<Conversation>(&path)?;
    sort_conversation_messages(&mut conversation);
    Ok(conversation)
}

#[tauri::command]
fn list_conversations(app: AppHandle) -> Result<Vec<Conversation>, String> {
    let root = conversations_root(&app)?;
    fs::create_dir_all(&root).map_err(to_string_error)?;
    let mut conversations = Vec::new();
    for entry in fs::read_dir(root).map_err(to_string_error)? {
        let entry = entry.map_err(to_string_error)?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let mut conversation = read_json::<Conversation>(&path)?;
        sort_conversation_messages(&mut conversation);
        conversations.push(conversation);
    }
    conversations.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(conversations)
}

#[tauri::command]
fn save_conversation(app: AppHandle, conversation: Conversation) -> Result<Conversation, String> {
    let path = conversation_path(&app, &conversation.id)?;
    let mut normalized = conversation;
    sort_conversation_messages(&mut normalized);
    write_json(&path, &normalized)?;
    write_conversation_summary(&app, &normalized)?;
    Ok(normalized)
}

#[tauri::command]
fn delete_conversation(app: AppHandle, conversation_id: String) -> Result<(), String> {
    let path = conversation_path(&app, &conversation_id)?;
    if path.exists() {
        fs::remove_file(path).map_err(to_string_error)?;
    }
    let summary_path = conversation_summary_path(&app, &conversation_id)?;
    if summary_path.exists() {
        fs::remove_file(summary_path).map_err(to_string_error)?;
    }
    Ok(())
}

#[tauri::command]
fn send_chat_message(
    app: AppHandle,
    conversation_id: String,
    request: ChatRequest,
) -> Result<(), String> {
    let settings = load_settings(app.clone())?;
    let conversation_path = conversation_path(&app, &conversation_id)?;

    thread::spawn(move || {
        let result = run_chat_request(
            &app,
            &conversation_path,
            &conversation_id,
            request,
            settings,
        );
        if let Err(error) = result {
            let _ = app.emit(
                "chat://error",
                ChatErrorEvent {
                    conversation_id,
                    error,
                },
            );
        }
    });

    Ok(())
}

fn run_chat_request(
    app: &AppHandle,
    conversation_path: &Path,
    conversation_id: &str,
    request: ChatRequest,
    settings: AppSettings,
) -> Result<(), String> {
    let mut conversation = if conversation_path.exists() {
        read_json::<Conversation>(conversation_path)?
    } else {
        Conversation {
            id: conversation_id.to_string(),
            title: "新对话".into(),
            created_at: now_iso(),
            updated_at: now_iso(),
            messages: Vec::new(),
        }
    };

    let now = now_iso();
    conversation.messages.push(ChatMessage {
        id: Uuid::new_v4().to_string(),
        role: "user".into(),
        content: request.content.clone(),
        created_at: now.clone(),
    });
    conversation.updated_at = now.clone();
    if conversation.title == "新对话" {
        conversation.title = request.content.chars().take(32).collect();
    }
    sort_conversation_messages(&mut conversation);
    write_json(conversation_path, &conversation)?;

    let assistant_content = run_agent_streaming_chat(app, &settings, &request, conversation_id)
        .or_else(|stream_error| {
            let fallback = run_agent_cli_chat(app, &settings, &request);
            match fallback {
                Ok(content) => {
                    if !content.is_empty() {
                        emit_streaming_chunks(app, conversation_id, &content)?;
                    }
                    Ok(content)
                }
                Err(fallback_error) => Err(format!("{stream_error}\n{fallback_error}")),
            }
        })?;

    conversation.messages.push(ChatMessage {
        id: Uuid::new_v4().to_string(),
        role: "assistant".into(),
        content: assistant_content,
        created_at: now_iso(),
    });
    conversation.updated_at = now_iso();
    sort_conversation_messages(&mut conversation);
    write_json(conversation_path, &conversation)?;
    app.emit(
        "chat://done",
        ChatDoneEvent {
            conversation_id: conversation_id.to_string(),
            conversation,
        },
    )
    .map_err(to_string_error)?;
    Ok(())
}

fn emit_conversation_update(app: &AppHandle, conversation: &Conversation) -> Result<(), String> {
    app.emit(
        "chat://conversation",
        ChatConversationEvent {
            conversation_id: conversation.id.clone(),
            conversation: conversation.clone(),
        },
    )
    .map_err(to_string_error)
}

fn emit_streaming_chunks(
    app: &AppHandle,
    conversation_id: &str,
    content: &str,
) -> Result<(), String> {
    let characters: Vec<char> = content.chars().collect();
    let chunk_size = if characters.len() > 180 {
        8
    } else if characters.len() > 60 {
        4
    } else {
        2
    };

    for chunk in characters.chunks(chunk_size) {
        let content: String = chunk.iter().collect();
        app.emit(
            "chat://delta",
            ChatDeltaEvent {
                conversation_id: conversation_id.to_string(),
                content,
            },
        )
        .map_err(to_string_error)?;
        thread::sleep(Duration::from_millis(24));
    }

    Ok(())
}

fn run_agent_streaming_chat(
    app: &AppHandle,
    settings: &AppSettings,
    request: &ChatRequest,
    conversation_id: &str,
) -> Result<String, String> {
    let executable = settings
        .openclaw_executable_path
        .clone()
        .ok_or_else(|| "未找到 OpenClaw 启动入口，请先到设置页确认。".to_string())?;
    let executable_path = PathBuf::from(&executable);
    if !executable_path.exists() {
        return Err("当前配置的 OpenClaw 启动入口不存在，请重新检测或手动选择。".into());
    }

    let sdk_path = resolve_acp_sdk_path(&executable_path)
        .ok_or_else(|| "当前 OpenClaw 安装不支持 ACP 真流式输出。".to_string())?;
    let helper_path = resolve_acp_helper_path(app)?;
    let node_path = resolve_node_executable(&executable_path)?;
    let profile_id = request
        .profile_id
        .clone()
        .or_else(|| settings.recent_profile_id.clone())
        .unwrap_or_else(|| LOCAL_PROFILE_ID.to_string());
    let launch_target = resolve_launch_target(app, settings, &profile_id)?;
    let runtime_state = app.state::<RuntimeState>();
    let gateway_config =
        ensure_target_gateway_running(&runtime_state, &executable_path, &launch_target)?;
    let gateway_token = read_profile_gateway_secret(&launch_target.profile_path, "token");
    let gateway_password = read_profile_gateway_secret(&launch_target.profile_path, "password");
    let current_dir = executable_path
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "无法确定 OpenClaw 的启动目录。".to_string())?;

    let gateway_ws_url = to_gateway_ws_url(&gateway_config.url);
    let payload = AcpStreamPayload {
        openclaw_path: executable_path.display().to_string(),
        cwd: current_dir.display().to_string(),
        profile_name: launch_target.cli_profile_name.clone(),
        session_key: String::new(),
        gateway_url: Some(gateway_ws_url.clone()),
        gateway_token,
        gateway_password,
        sdk_path: sdk_path.display().to_string(),
        message: request.content.clone(),
    };

    let payload_path = write_acp_payload_file(app, &payload)?;
    let mut command = Command::new(node_path);
    apply_windows_process_flags(&mut command);
    command
        .arg(helper_path)
        .arg(&payload_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(current_dir);

    let mut child = command.spawn().map_err(to_string_error)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法读取 ACP 流式输出。".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "无法读取 ACP 错误输出。".to_string())?;

    let stderr_handle = thread::spawn(move || {
        let mut stderr_text = String::new();
        let mut reader = BufReader::new(stderr);
        let _ = reader.read_to_string(&mut stderr_text);
        stderr_text
    });

    let mut assistant_content = String::new();
    let mut saw_done = false;
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(to_string_error)?;
        if line.trim().is_empty() {
            continue;
        }
        let event: AcpStreamEvent = serde_json::from_str(&line)
            .map_err(|error| format!("无法解析 ACP 流式消息：{error}"))?;
        match event {
            AcpStreamEvent::Delta { content } => {
                assistant_content.push_str(&content);
                app.emit(
                    "chat://delta",
                    ChatDeltaEvent {
                        conversation_id: conversation_id.to_string(),
                        content,
                    },
                )
                .map_err(to_string_error)?;
            }
            AcpStreamEvent::Done { .. } => {
                saw_done = true;
            }
            AcpStreamEvent::Error { error } => {
                let _ = fs::remove_file(&payload_path);
                return Err(format!(
                    "{error}\nGateway URL: {}\nGateway WS URL: {}",
                    gateway_config.url, gateway_ws_url
                ));
            }
            AcpStreamEvent::Tool { .. } | AcpStreamEvent::ToolUpdate { .. } => {}
        }
    }

    let status = child.wait().map_err(to_string_error)?;
    let stderr_text = stderr_handle.join().unwrap_or_default();
    let _ = fs::remove_file(&payload_path);

    if !status.success() && assistant_content.trim().is_empty() {
        return Err(format!(
            "{}\nGateway URL: {}\nGateway WS URL: {}",
            agent_cli_error_message("", &stderr_text),
            gateway_config.url,
            gateway_ws_url
        ));
    }
    if !saw_done && assistant_content.trim().is_empty() {
        return Err(if stderr_text.trim().is_empty() {
            "龙虾没有返回流式内容。".to_string()
        } else {
            stderr_text
        });
    }
    if assistant_content.trim().is_empty() {
        Ok("龙虾暂时没有返回内容.".to_string())
    } else {
        Ok(assistant_content)
    }
}

fn run_agent_cli_chat(
    app: &AppHandle,
    settings: &AppSettings,
    request: &ChatRequest,
) -> Result<String, String> {
    let executable = settings
        .openclaw_executable_path
        .clone()
        .ok_or_else(|| "未找到 OpenClaw 启动入口，请先到设置页确认。".to_string())?;
    let executable_path = PathBuf::from(&executable);
    if !executable_path.exists() {
        return Err("当前配置的 OpenClaw 启动入口不存在，请重新检测或手动选择。".into());
    }

    let profile_id = request
        .profile_id
        .clone()
        .or_else(|| settings.recent_profile_id.clone())
        .unwrap_or_else(|| LOCAL_PROFILE_ID.to_string());
    let launch_target = resolve_launch_target(app, settings, &profile_id)?;
    let runtime_state = app.state::<RuntimeState>();
    let gateway_config =
        ensure_target_gateway_running(&runtime_state, &executable_path, &launch_target)?;

    let mut command = build_openclaw_command(&executable_path);
    if let Some(profile_name) = launch_target.cli_profile_name {
        command.arg("--profile").arg(profile_name);
    } else if launch_target.use_state_dir_env {
        command.env("OPENCLAW_STATE_DIR", &launch_target.profile_path);
    }
    apply_gateway_env(&mut command, &gateway_config, &launch_target.profile_path);
    command
        .arg("--no-color")
        .arg("agent")
        .arg("--session-id")
        .arg(&request.conversation_id)
        .arg("--message")
        .arg(&request.content)
        .arg("--json")
        .current_dir(
            executable_path
                .parent()
                .map(PathBuf::from)
                .ok_or_else(|| "无法确定 OpenClaw 的启动目录。".to_string())?,
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = command.output().map_err(to_string_error)?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    let parsed_response = parse_agent_cli_json(&stdout);
    let parsed_content = parsed_response
        .as_ref()
        .map(extract_agent_cli_text)
        .unwrap_or_default();
    if !parsed_content.trim().is_empty() {
        return Ok(parsed_content);
    }

    if !output.status.success() {
        return Err(agent_cli_error_message(&stdout, &stderr));
    }

    let response = parsed_response.ok_or_else(|| {
        format!(
            "无法解析龙虾回复.\n{}",
            raw_output_excerpt(&stdout, &stderr)
        )
    })?;
    let content = extract_agent_cli_text(&response);
    if content.trim().is_empty() {
        Ok("龙虾暂时没有返回内容.".to_string())
    } else {
        Ok(content)
    }
}

fn parse_agent_cli_json(stdout: &str) -> Option<serde_json::Value> {
    let trimmed = stdout.trim();
    serde_json::from_str(trimmed).ok().or_else(|| {
        let start = trimmed.find('{')?;
        let end = trimmed.rfind('}')?;
        serde_json::from_str(&trimmed[start..=end]).ok()
    })
}

fn extract_agent_cli_text(response: &serde_json::Value) -> String {
    let mut parts = Vec::new();

    if let Some(payloads) = response
        .get("result")
        .and_then(|value| value.get("payloads"))
        .and_then(|value| value.as_array())
    {
        for payload in payloads {
            if let Some(text) = payload.get("text").and_then(|value| value.as_str()) {
                let text = text.trim();
                if !text.is_empty() {
                    parts.push(text.to_string());
                }
            }

            if let Some(url) = payload.get("mediaUrl").and_then(|value| value.as_str()) {
                let url = url.trim();
                if !url.is_empty() {
                    parts.push(format!("媒体：{url}"));
                }
            }

            if let Some(urls) = payload.get("mediaUrls").and_then(|value| value.as_array()) {
                for url in urls.iter().filter_map(|value| value.as_str()) {
                    let url = url.trim();
                    if !url.is_empty() {
                        parts.push(format!("媒体：{url}"));
                    }
                }
            }
        }
    }

    if parts.is_empty() {
        if let Some(payloads) = response.get("payloads").and_then(|value| value.as_array()) {
            for payload in payloads {
                if let Some(text) = payload.get("text").and_then(|value| value.as_str()) {
                    let text = text.trim();
                    if !text.is_empty() {
                        parts.push(text.to_string());
                    }
                }

                if let Some(url) = payload.get("mediaUrl").and_then(|value| value.as_str()) {
                    let url = url.trim();
                    if !url.is_empty() {
                        parts.push(format!("媒体：{url}"));
                    }
                }

                if let Some(urls) = payload.get("mediaUrls").and_then(|value| value.as_array()) {
                    for url in urls.iter().filter_map(|value| value.as_str()) {
                        let url = url.trim();
                        if !url.is_empty() {
                            parts.push(format!("媒体：{url}"));
                        }
                    }
                }
            }
        }
    }

    if parts.is_empty() {
        if let Some(summary) = response.get("summary").and_then(|value| value.as_str()) {
            let summary = summary.trim();
            if !summary.is_empty() {
                parts.push(summary.to_string());
            }
        }
    }

    parts.join("\n\n")
}

fn agent_cli_error_message(stdout: &str, stderr: &str) -> String {
    let stderr = stderr.trim();
    if !stderr.is_empty() {
        return stderr.to_string();
    }

    let stdout = stdout.trim();
    if !stdout.is_empty() {
        return stdout.to_string();
    }

    "龙虾回复失败.".to_string()
}

fn raw_output_excerpt(stdout: &str, stderr: &str) -> String {
    let mut parts = Vec::new();
    if !stderr.trim().is_empty() {
        parts.push(stderr.trim());
    }
    if !stdout.trim().is_empty() {
        parts.push(stdout.trim());
    }
    if parts.is_empty() {
        "没有收到可用输出.".to_string()
    } else {
        parts.join("\n")
    }
}

#[allow(unreachable_code)]
fn validate_paths(executable_path: Option<String>, data_dir: Option<String>) -> ValidationResult {
    validate_paths_impl(executable_path, data_dir)
}

fn common_openclaw_candidates() -> Vec<(String, i32)> {
    let mut candidates = Vec::new();
    let executable_name = if cfg!(target_os = "windows") {
        "openclaw.exe"
    } else {
        "openclaw"
    };

    if cfg!(target_os = "windows") {
        if let Ok(appdata) = env::var("APPDATA") {
            candidates.push((
                PathBuf::from(&appdata)
                    .join("npm")
                    .join("openclaw.cmd")
                    .display()
                    .to_string(),
                115,
            ));
            candidates.push((
                PathBuf::from(&appdata)
                    .join("npm")
                    .join("openclaw")
                    .display()
                    .to_string(),
                110,
            ));
        }
        if let Some(user_home) = default_user_home() {
            candidates.push((
                user_home
                    .join("AppData")
                    .join("Roaming")
                    .join("npm")
                    .join("openclaw.cmd")
                    .display()
                    .to_string(),
                112,
            ));
        }
        for candidate in where_openclaw_candidates() {
            candidates.push((candidate.display().to_string(), 118));
        }
        for (var, score) in [
            ("ProgramFiles", 100),
            ("LOCALAPPDATA", 90),
            ("ProgramFiles(x86)", 80),
        ] {
            if let Ok(base) = env::var(var) {
                candidates.push((
                    PathBuf::from(&base)
                        .join("OpenClaw")
                        .join(executable_name)
                        .display()
                        .to_string(),
                    score,
                ));
            }
        }
    } else if cfg!(target_os = "macos") {
        for (candidate, score) in [
            ("/Applications/OpenClaw.app/Contents/MacOS/OpenClaw", 120),
            ("/Applications/OpenClaw.app/Contents/MacOS/openclaw", 118),
            ("/opt/homebrew/bin/openclaw", 116),
            ("/usr/local/bin/openclaw", 114),
            ("/usr/bin/openclaw", 108),
        ] {
            candidates.push((candidate.into(), score));
        }
        if let Ok(home) = env::var("HOME") {
            candidates.push((
                PathBuf::from(&home)
                    .join("Applications/OpenClaw.app/Contents/MacOS/OpenClaw")
                    .display()
                    .to_string(),
                112,
            ));
            candidates.push((
                PathBuf::from(&home)
                    .join("Applications/OpenClaw.app/Contents/MacOS/openclaw")
                    .display()
                    .to_string(),
                110,
            ));
            candidates.push((
                PathBuf::from(&home)
                    .join(".local/bin/openclaw")
                    .display()
                    .to_string(),
                106,
            ));
        }
    } else {
        for (candidate, score) in [
            ("/usr/local/bin/openclaw", 100),
            ("/usr/bin/openclaw", 90),
            ("/opt/openclaw/openclaw", 80),
        ] {
            candidates.push((candidate.into(), score));
        }
        if let Ok(home) = env::var("HOME") {
            candidates.push((
                PathBuf::from(home)
                    .join(".local/bin/openclaw")
                    .display()
                    .to_string(),
                95,
            ));
        }
    }
    candidates
}

fn infer_data_dir(executable_path: &Path) -> Option<PathBuf> {
    if let Some(default_path) = default_openclaw_data_dir_path() {
        return Some(default_path);
    }
    let candidates = [
        executable_path.parent().map(|path| path.join(".openclaw")),
        executable_path.parent().map(|path| path.join("openclaw")),
        executable_path.parent().map(|path| path.join("data")),
        executable_path
            .parent()
            .and_then(|path| path.parent())
            .map(|path| path.join(".openclaw")),
        executable_path
            .parent()
            .and_then(|path| path.parent())
            .map(|path| path.join("openclaw")),
    ];
    for candidate in candidates.into_iter().flatten() {
        if looks_like_openclaw_data_dir(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn export_profile_impl(request: ExportProfileRequest) -> Result<PackageMeta, String> {
    let source = PathBuf::from(&request.source_dir);
    if !source.is_dir() {
        return Err("要导出的资料目录不存在.".into());
    }

    let include_memory = request.include_memory.unwrap_or(false);
    let include_account_info = request.include_account_info.unwrap_or(false);
    let package_name = request
        .package_name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
    let target = request
        .zip_path
        .map(PathBuf::from)
        .unwrap_or_else(|| source.with_extension("claw"));
    ensure_parent(&target)?;

    let file = File::create(&target).map_err(to_string_error)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let manifest_entries = collect_manifest_entries(&source, include_memory, include_account_info)?;
    let manifest = PackageManifest {
        format_version: 2,
        package_name: package_name.clone(),
        exported_at: now_iso(),
        source_dir_name: source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        version: None,
        include_memory,
        include_account_info,
        entries: manifest_entries,
    };
    zip.start_file(MANIFEST_FILE, options)
        .map_err(to_string_error)?;
    zip.write_all(
        serde_json::to_string_pretty(&manifest)
            .map_err(to_string_error)?
            .as_bytes(),
    )
    .map_err(to_string_error)?;

    let mut file_count = 0usize;
    for entry in WalkDir::new(&source) {
        let entry = entry.map_err(to_string_error)?;
        let path = entry.path();
        if path == source {
            continue;
        }

        let relative = path.strip_prefix(&source).map_err(to_string_error)?;
        let relative_path = relative.to_string_lossy().replace('\\', "/");
        if should_skip_export_path(&relative_path, include_memory, include_account_info)
            || relative_path == MANIFEST_FILE
        {
            continue;
        }

        if entry.file_type().is_dir() {
            zip.add_directory(relative_path, options)
                .map_err(to_string_error)?;
            continue;
        }

        zip.start_file(relative_path, options)
            .map_err(to_string_error)?;
        let mut input = File::open(path).map_err(to_string_error)?;
        std::io::copy(&mut input, &mut zip).map_err(to_string_error)?;
        file_count += 1;
    }

    zip.finish().map_err(to_string_error)?;
    Ok(PackageMeta {
        package_name,
        zip_path: target.display().to_string(),
        source_dir: source.display().to_string(),
        exported_at: now_iso(),
        file_count,
        include_memory,
        include_account_info,
    })
}

fn collect_manifest_entries(
    source: &Path,
    include_memory: bool,
    include_account_info: bool,
) -> Result<Vec<ManifestEntry>, String> {
    let mut entries = Vec::new();
    for entry in WalkDir::new(source) {
        let entry = entry.map_err(to_string_error)?;
        let path = entry.path();
        if path == source || entry.file_type().is_dir() {
            continue;
        }

        let relative = path.strip_prefix(source).map_err(to_string_error)?;
        let relative_path = relative.to_string_lossy().replace('\\', "/");
        if should_skip_export_path(&relative_path, include_memory, include_account_info)
            || relative_path == MANIFEST_FILE
        {
            continue;
        }

        let metadata = fs::metadata(path).map_err(to_string_error)?;
        entries.push(ManifestEntry {
            path: relative_path,
            size: metadata.len(),
            sha256: sha256_file(path)?,
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

fn should_skip_export_path(
    relative_path: &str,
    include_memory: bool,
    include_account_info: bool,
) -> bool {
    let normalized = relative_path.trim_start_matches("./");
    if normalized.is_empty() {
        return false;
    }

    if !include_memory
        && (normalized.starts_with("memory/")
            || normalized.contains("/memory/")
            || normalized.starts_with("sessions/")
            || normalized.contains("/sessions/")
            || normalized.eq_ignore_ascii_case("USER.md")
            || normalized.ends_with("/USER.md"))
    {
        return true;
    }

    if !include_account_info
        && (normalized.starts_with("identity/")
            || normalized.starts_with("devices/")
            || normalized.ends_with("/auth-profiles.json")
            || normalized == "auth-profiles.json")
    {
        return true;
    }

    false
}

fn pick_openclaw_executable_impl() -> Option<String> {
    FileDialog::new()
        .set_title(if is_english_build() {
            "Choose OpenClaw executable"
        } else {
            "选择 OpenClaw 启动入口"
        })
        .pick_file()
        .and_then(|path| normalize_openclaw_command_path(&path).or(Some(path)))
        .map(|path| path.display().to_string())
}

fn pick_directory_impl() -> Option<String> {
    FileDialog::new()
        .set_title(if is_english_build() {
            "Choose directory"
        } else {
            "选择目录"
        })
        .pick_folder()
        .map(|path| path.display().to_string())
}

fn validate_paths_impl(
    executable_path: Option<String>,
    data_dir: Option<String>,
) -> ValidationResult {
    let mut issues = Vec::new();
    let executable = executable_path
        .map(PathBuf::from)
        .and_then(|path| normalize_openclaw_command_path(&path).or(Some(path)))
        .filter(|path| path.exists());
    if executable.is_none() {
        issues.push("未找到启动入口.".into());
    }

    let inferred_data_dir = data_dir
        .map(PathBuf::from)
        .filter(|path| looks_like_openclaw_data_dir(path))
        .or_else(default_openclaw_data_dir_path)
        .or_else(|| executable.as_ref().and_then(|path| infer_data_dir(path)));
    if inferred_data_dir.is_none() {
        issues.push("未找到可用的数据目录.".into());
    }

    let supports_profile_switch = executable
        .as_ref()
        .map(|path| command_supports_profile_switch(path))
        .unwrap_or(false);
    if !supports_profile_switch {
        issues.push("无法确认当前入口是否支持 --profile.".into());
    }

    ValidationResult {
        executable_path: executable.as_ref().map(display),
        install_dir: executable
            .as_ref()
            .and_then(|path| path.parent().map(display_path)),
        inferred_data_dir: inferred_data_dir.as_ref().map(display),
        supports_profile_switch,
        is_valid: executable.is_some() && inferred_data_dir.is_some(),
        issues,
    }
}

fn list_profiles_impl(app: AppHandle) -> Result<Vec<ManagedProfile>, String> {
    let settings = load_settings(app.clone())?;
    let root = profiles_root(&app, &settings)?;
    fs::create_dir_all(&root).map_err(to_string_error)?;
    let mut profiles = Vec::new();
    for entry in fs::read_dir(root).map_err(to_string_error)? {
        let entry = entry.map_err(to_string_error)?;
        let path = entry.path();
        if path.is_dir() && is_managed_profile_dir(&path) && path.join(PROFILE_META_FILE).is_file()
        {
            profiles.push(load_profile_metadata_impl(&path)?);
        }
    }
    profiles.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(profiles)
}

fn load_profile_metadata_impl(path: &Path) -> Result<ManagedProfile, String> {
    let meta_path = path.join(PROFILE_META_FILE);
    if meta_path.exists() {
        let meta = read_json::<ProfileMeta>(&meta_path)?;
        return Ok(ManagedProfile {
            id: meta.id,
            name: if meta.name.trim().is_empty() {
                managed_profile_name_from_dir(path)
            } else {
                meta.name
            },
            path: path.display().to_string(),
            imported_from: meta.imported_from,
            created_at: meta.created_at,
            last_used_at: meta.last_used_at,
        });
    }

    let created = fs::metadata(path)
        .and_then(|meta| meta.created().or_else(|_| meta.modified()))
        .unwrap_or(SystemTime::now());
    Ok(ManagedProfile {
        id: Uuid::new_v4().to_string(),
        name: managed_profile_name_from_dir(path),
        path: path.display().to_string(),
        imported_from: None,
        created_at: system_time_to_iso(created),
        last_used_at: None,
    })
}

fn launch_openclaw_impl(
    app: AppHandle,
    profile_id: String,
    _state: State<'_, RuntimeState>,
) -> Result<LaunchHandle, String> {
    let mut settings = load_settings(app.clone())?;
    let executable = settings
        .openclaw_executable_path
        .clone()
        .ok_or_else(|| "未找到 OpenClaw 启动入口，请先到设置页确认。".to_string())?;
    let executable_path = PathBuf::from(&executable);
    if !executable_path.exists() {
        return Err("当前配置的启动入口不存在，请重新检测或手动选择。".into());
    }

    let launch_target = resolve_launch_target(&app, &settings, &profile_id)?;
    let gateway_config = gateway_config_for_target(&executable_path, &launch_target)?;
    let gateway_ready = health_check(&gateway_config).is_ok();
    if !gateway_ready {
        let background_app = app.clone();
        let background_executable = executable_path.clone();
        let background_target = launch_target.clone();
        thread::spawn(move || {
            let runtime_state = background_app.state::<RuntimeState>();
            let _ = ensure_target_gateway_running(
                &runtime_state,
                &background_executable,
                &background_target,
            );
        });
    }
    let started_at = now_iso();
    let mut command = build_openclaw_command(&executable_path);
    if let Some(profile_name) = launch_target.cli_profile_name.clone() {
        command.arg("--profile").arg(&profile_name);
    } else if launch_target.use_state_dir_env {
        command.env("OPENCLAW_STATE_DIR", &launch_target.profile_path);
    }
    command.arg("dashboard").arg("--no-open");
    command
        .current_dir(
            executable_path
                .parent()
                .map(PathBuf::from)
                .ok_or_else(|| "无法确定 OpenClaw 的启动目录。".to_string())?,
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_gateway_env(&mut command, &gateway_config, &launch_target.profile_path);

    let child = command.spawn().map_err(to_string_error)?;
    let mut args = Vec::new();
    if let Some(profile_name) = launch_target.cli_profile_name.clone() {
        args.push("--profile".into());
        args.push(profile_name);
    }
    args.push("dashboard".into());
    args.push("--no-open".into());

    let handle = LaunchHandle {
        pid: Some(child.id()),
        started_at: started_at.clone(),
        profile_id: launch_target.profile_id.clone(),
        profile_name: launch_target.profile_name.clone(),
        executable_path: executable.clone(),
        args,
        connection_message: None,
    };

    settings.recent_profile_id = Some(launch_target.profile_id.clone());
    settings.gateway_config = gateway_config.clone();
    settings.recent_launches.insert(
        0,
        LaunchRecord {
            profile_id: launch_target.profile_id.clone(),
            profile_name: launch_target.profile_name.clone(),
            launched_at: started_at,
        },
    );
    settings.recent_launches.truncate(10);
    let connection_message = if gateway_ready {
        Some("已复用当前连接服务.".to_string())
    } else {
        Some("正在启动这只龙虾自己的连接服务。".to_string())
    };
    let saved_settings = save_settings(app.clone(), settings)?;
    let _ = ensure_gateway_subscriber(&app, &saved_settings);

    if let Some(profile) = launch_target.managed_profile {
        let profile_path = PathBuf::from(&profile.path);
        write_json(
            &profile_path.join(PROFILE_META_FILE),
            &ProfileMeta {
                id: profile.id.clone(),
                name: profile.name.clone(),
                imported_from: profile.imported_from.clone(),
                created_at: profile.created_at.clone(),
                last_used_at: Some(now_iso()),
            },
        )?;
    }

    Ok(LaunchHandle {
        connection_message,
        ..handle
    })
}

fn open_control_web_impl(
    app: AppHandle,
    profile_id: String,
    state: State<'_, RuntimeState>,
) -> Result<(), String> {
    let settings = load_settings(app.clone())?;
    let executable = settings
        .openclaw_executable_path
        .clone()
        .ok_or_else(|| "未找到 OpenClaw 启动入口，请先到设置页确认。".to_string())?;
    let executable_path = PathBuf::from(&executable);
    if !executable_path.exists() {
        return Err("当前配置的启动入口不存在，请重新检测或手动选择。".into());
    }

    let launch_target = resolve_launch_target(&app, &settings, &profile_id)?;
    let gateway_config = ensure_target_gateway_running(&state, &executable_path, &launch_target)?;
    let url = control_web_url(&gateway_config, &launch_target.profile_path);

    if cfg!(target_os = "windows") {
        let mut command = Command::new("rundll32");
        apply_windows_process_flags(&mut command);
        command
            .arg("url.dll,FileProtocolHandler")
            .arg(&url)
            .spawn()
            .map_err(to_string_error)?;
    } else if cfg!(target_os = "macos") {
        Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(to_string_error)?;
    } else {
        Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(to_string_error)?;
    }

    Ok(())
}

fn start_gateway_impl(
    mode: String,
    gateway_config: GatewayConfig,
    state: State<'_, RuntimeState>,
) -> Result<GatewayStatus, String> {
    let mut runtime = state
        .gateway
        .lock()
        .map_err(|_| "无法读取连接状态.".to_string())?;
    let mut status = GatewayStatus {
        mode: mode.clone(),
        url: gateway_config.url.clone(),
        running: false,
        pid: None,
        started_at: None,
        healthy: false,
        last_error: None,
        log_tail: vec![],
    };

    if !gateway_config.url.trim().is_empty() && health_check(&gateway_config).is_ok() {
        status.running = true;
        status.healthy = true;
        status.started_at = Some(now_iso());
        status.log_tail.push("检测到现有连接服务.".into());
        runtime.status = status.clone();
        return Ok(status);
    }

    if !gateway_config.url.trim().is_empty() && gateway_port_is_open(&gateway_config) {
        status.running = true;
        status.healthy = false;
        status.started_at = Some(now_iso());
        status.last_error = Some("检测到连接端口已被占用，已跳过重复启动.".into());
        status
            .log_tail
            .push("检测到连接端口已被占用，已跳过重复启动.".into());
        runtime.status = status.clone();
        return Ok(status);
    }

    if let Some(child) = runtime.child.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }
    runtime.child = None;

    if mode == "manual" {
        status.healthy = health_check(&gateway_config).is_ok();
        if !status.healthy {
            status.last_error = Some("没有连上连接服务，请确认地址和健康检查接口.".into());
        }
        runtime.status = status.clone();
        return Ok(status);
    }

    let command = gateway_config
        .command
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "自动启动连接服务时需要填写命令路径.".to_string())?;
    let command_path = PathBuf::from(&command);
    if !command_path.exists() {
        return Err("自动启动连接服务的命令不存在.".into());
    }

    let mut process = build_openclaw_command(&command_path);
    process
        .args(&gateway_config.args)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(parent) = command_path.parent() {
        process.current_dir(parent);
    }

    let child = process.spawn().map_err(to_string_error)?;
    status.running = true;
    status.pid = Some(child.id());
    status.started_at = Some(now_iso());
    status
        .log_tail
        .push(format!("宸插惎鍔ㄨ繛鎺ユ湇鍔″懡浠わ細{command}"));
    status.healthy =
        health_check_with_retry(&gateway_config, 6, Duration::from_millis(800)).is_ok();
    if !status.healthy {
        status.last_error = Some("连接服务已启动，但健康检查没有通过.".into());
    }

    runtime.child = Some(child);
    runtime.status = status.clone();
    Ok(status)
}

fn stop_gateway_impl(state: State<'_, RuntimeState>) -> Result<GatewayStatus, String> {
    let mut runtime = state
        .gateway
        .lock()
        .map_err(|_| "无法读取连接状态.".to_string())?;
    if let Some(child) = runtime.child.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }
    runtime.child = None;
    runtime.status.running = false;
    runtime.status.healthy = false;
    runtime.status.pid = None;
    runtime.status.started_at = None;
    runtime.probe_in_flight = false;
    runtime.last_probe_at = Some(SystemTime::now());
    runtime.status.log_tail.push("连接服务已停止.".into());
    runtime.status.log_tail = tail(runtime.status.log_tail.clone(), 10);
    Ok(runtime.status.clone())
}

fn gateway_status_impl(
    _app: AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<GatewayStatus, String> {
    let status = {
        let mut runtime = state
            .gateway
            .lock()
            .map_err(|_| "无法读取连接状态.".to_string())?;
        let exit_status = if let Some(child) = runtime.child.as_mut() {
            child.try_wait().map_err(to_string_error)?
        } else {
            None
        };
        if let Some(exit) = exit_status {
            runtime.status.running = false;
            runtime.status.healthy = false;
            runtime.status.pid = None;
            runtime.status.last_error = Some(format!("连接服务已退出：{exit}."));
            runtime.child = None;
            runtime.last_probe_at = Some(SystemTime::now());
        }
        runtime.status.clone()
    };
    Ok(status)
}

fn start_gateway_status_poller(app: AppHandle) {
    thread::spawn(move || loop {
        refresh_gateway_status_cache(&app);
        thread::sleep(Duration::from_secs(5));
    });
}

fn refresh_gateway_status_cache(app: &AppHandle) {
    {
        let runtime_state = app.state::<RuntimeState>();
        let mut runtime = match runtime_state.gateway.lock() {
            Ok(runtime) => runtime,
            Err(_) => return,
        };
        if runtime.probe_in_flight {
            return;
        }
        runtime.probe_in_flight = true;
    }

    let config = {
        let runtime_state = app.state::<RuntimeState>();
        let runtime = match runtime_state.gateway.lock() {
            Ok(runtime) => runtime,
            Err(_) => return,
        };
        GatewayConfig {
            mode: runtime.status.mode.clone(),
            command: None,
            args: Vec::new(),
            url: runtime.status.url.clone(),
            health_endpoint: "/health".to_string(),
        }
    };

    let healthy = if config.url.trim().is_empty() {
        false
    } else {
        health_check(&config).is_ok()
    };
    let port_open = if config.url.trim().is_empty() {
        false
    } else {
        gateway_port_is_open(&config)
    };

    if let Ok(mut runtime) = app.state::<RuntimeState>().gateway.lock() {
        runtime.status.mode = config.mode.clone();
        runtime.status.url = config.url.clone();
        runtime.status.healthy = healthy;
        runtime.status.running = healthy || runtime.child.is_some() || port_open;
        if healthy {
            runtime.status.last_error = None;
        }
        runtime.probe_in_flight = false;
        runtime.last_probe_at = Some(SystemTime::now());
    }
}

fn sync_runtime_gateway_defaults(app: &AppHandle, settings: &AppSettings) {
    if let Ok(mut runtime) = app.state::<RuntimeState>().gateway.lock() {
        runtime.status.mode = settings.gateway_config.mode.clone();
        runtime.status.url = settings.gateway_config.url.clone();
    }
}

fn sort_conversation_messages(conversation: &mut Conversation) {
    conversation.messages.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn gateway_url_for_port(port: u64) -> String {
    format!("http://127.0.0.1:{port}")
}

fn control_web_url(gateway_config: &GatewayConfig, profile_path: &str) -> String {
    let mut url = gateway_config.url.trim_end_matches('/').to_string();
    if let Some(token) = read_profile_gateway_secret(profile_path, "token") {
        url.push_str("/#token=");
        url.push_str(&token);
    }
    url
}

fn gateway_config_for_target(
    executable_path: &Path,
    launch_target: &LaunchTarget,
) -> Result<GatewayConfig, String> {
    let profile_root = Path::new(&launch_target.profile_path);
    let port = read_gateway_port(profile_root)
        .ok_or_else(|| "没有找到这只龙虾的连接端口配置。".to_string())?;
    let mut args = Vec::new();
    if let Some(profile_name) = launch_target.cli_profile_name.clone() {
        args.push("--profile".into());
        args.push(profile_name);
    }
    args.push("gateway".into());
    args.push("run".into());
    args.push("--port".into());
    args.push(port.to_string());

    if let Some((mode, secret)) = read_gateway_auth_secret(profile_root) {
        if mode == "token" {
            args.push("--token".into());
            args.push(secret);
        } else if mode == "password" {
            args.push("--password".into());
            args.push(secret);
        }
    }

    Ok(GatewayConfig {
        mode: "auto".into(),
        command: Some(executable_path.display().to_string()),
        args,
        url: read_gateway_remote_url(profile_root).unwrap_or_else(|| gateway_url_for_port(port)),
        health_endpoint: "/health".into(),
    })
}

fn ensure_target_gateway_running(
    state: &State<'_, RuntimeState>,
    executable_path: &Path,
    launch_target: &LaunchTarget,
) -> Result<GatewayConfig, String> {
    let gateway_config = gateway_config_for_target(executable_path, launch_target)?;

    if health_check(&gateway_config).is_ok() {
        let mut runtime = state
            .gateway
            .lock()
            .map_err(|_| "无法读取连接状态.".to_string())?;
        runtime.status.mode = gateway_config.mode.clone();
        runtime.status.url = gateway_config.url.clone();
        runtime.status.running = true;
        runtime.status.healthy = true;
        runtime.status.last_error = None;
        runtime.status.log_tail = tail(runtime.status.log_tail.clone(), 10);
        return Ok(gateway_config);
    }

    let command = gateway_config
        .command
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "自动启动连接服务时需要可用的 OpenClaw 启动入口。".to_string())?;
    let command_path = PathBuf::from(&command);
    if !command_path.exists() {
        return Err("用于启动这只龙虾连接服务的命令不存在。".into());
    }

    {
        let mut runtime = state
            .gateway
            .lock()
            .map_err(|_| "无法读取连接状态.".to_string())?;
        if let Some(child) = runtime.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        runtime.child = None;
    }

    let mut process = build_openclaw_command(&command_path);
    process
        .args(&gateway_config.args)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if launch_target.use_state_dir_env {
        process.env("OPENCLAW_STATE_DIR", &launch_target.profile_path);
    }
    if let Some(parent) = command_path.parent() {
        process.current_dir(parent);
    }

    let child = process.spawn().map_err(to_string_error)?;
    let pid = child.id();
    {
        let mut runtime = state
            .gateway
            .lock()
            .map_err(|_| "无法读取连接状态.".to_string())?;
        runtime.child = Some(child);
        runtime.status = GatewayStatus {
            mode: gateway_config.mode.clone(),
            url: gateway_config.url.clone(),
            running: true,
            pid: Some(pid),
            started_at: Some(now_iso()),
            healthy: false,
            last_error: None,
            log_tail: vec![format!("已为当前龙虾启动连接服务：{command}")],
        };
    }

    let healthy = health_check_with_retry(&gateway_config, 8, Duration::from_millis(600)).is_ok();
    let mut runtime = state
        .gateway
        .lock()
        .map_err(|_| "无法读取连接状态.".to_string())?;
    runtime.status.healthy = healthy;
    if healthy {
        runtime.status.last_error = None;
        Ok(gateway_config)
    } else {
        runtime.status.last_error = Some("这只龙虾自己的连接服务没有启动成功。".into());
        Err("这只龙虾自己的连接服务没有启动成功。".into())
    }
}

fn apply_gateway_env(command: &mut Command, gateway_config: &GatewayConfig, profile_path: &str) {
    if !gateway_config.url.trim().is_empty() {
        command.env("OPENCLAW_GATEWAY_URL", gateway_config.url.trim());
        command.env("CLAWDBOT_GATEWAY_URL", gateway_config.url.trim());
        if let Some((mode, secret)) = read_gateway_auth_secret(Path::new(profile_path)) {
            if mode == "token" {
                command.env("OPENCLAW_GATEWAY_TOKEN", &secret);
                command.env("CLAWDBOT_GATEWAY_TOKEN", &secret);
            } else if mode == "password" {
                command.env("OPENCLAW_GATEWAY_PASSWORD", &secret);
                command.env("CLAWDBOT_GATEWAY_PASSWORD", &secret);
            }
        }
    }
}

fn read_gateway_auth_secret(data_dir: &Path) -> Option<(String, String)> {
    let config_path = data_dir.join("openclaw.json");
    let config = read_json::<OpenClawFileConfig>(&config_path).ok()?;
    let auth = config.gateway?.auth?;
    let mode = auth.mode?.trim().to_lowercase();
    if mode == "token" {
        let token = auth.token?.trim().to_string();
        if !token.is_empty() {
            return Some((mode, token));
        }
    }
    if mode == "password" {
        let password = auth.password?.trim().to_string();
        if !password.is_empty() {
            return Some((mode, password));
        }
    }
    None
}

fn read_profile_gateway_secret(data_dir: &str, secret_kind: &str) -> Option<String> {
    let config_path = Path::new(data_dir).join("openclaw.json");
    let config = read_json::<OpenClawFileConfig>(&config_path).ok()?;
    let auth = config.gateway?.auth?;
    let value = if secret_kind == "token" {
        auth.token
    } else {
        auth.password
    }?;
    let value = value.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn to_gateway_ws_url(url: &str) -> String {
    let trimmed = url.trim();
    if let Some(value) = trimmed.strip_prefix("http://") {
        format!("ws://{value}")
    } else if let Some(value) = trimmed.strip_prefix("https://") {
        format!("wss://{value}")
    } else {
        trimmed.to_string()
    }
}

fn acp_session_key(profile_id: &str, conversation_id: &str) -> String {
    let profile = sanitize_session_key_part(profile_id);
    let conversation = sanitize_session_key_part(conversation_id);
    format!("launcher:{profile}:{conversation}")
}

fn sanitize_session_key_part(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    sanitized.trim_matches('-').to_string()
}

fn resolve_openclaw_module_root(executable_path: &Path) -> Option<PathBuf> {
    let executable_dir = executable_path.parent()?;
    let module_root = executable_dir.join("node_modules").join("openclaw");
    module_root.exists().then_some(module_root)
}

fn resolve_acp_sdk_path(executable_path: &Path) -> Option<PathBuf> {
    let module_root = resolve_openclaw_module_root(executable_path)?;
    let sdk_path = module_root
        .join("node_modules")
        .join("@agentclientprotocol")
        .join("sdk")
        .join("dist")
        .join("acp.js");
    sdk_path.exists().then_some(sdk_path)
}

fn resolve_ws_module_path(executable_path: &Path) -> Option<PathBuf> {
    let module_root = resolve_openclaw_module_root(executable_path)?;
    let ws_path = module_root
        .join("node_modules")
        .join("ws")
        .join("wrapper.mjs");
    ws_path.exists().then_some(ws_path)
}

fn resolve_node_executable(executable_path: &Path) -> Result<PathBuf, String> {
    if executable_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("exe"))
        .unwrap_or(false)
    {
        return Ok(executable_path.to_path_buf());
    }

    let executable_dir = executable_path
        .parent()
        .ok_or_else(|| "无法确定 OpenClaw 的启动目录。".to_string())?;
    let bundled_node = executable_dir.join("node.exe");
    if bundled_node.exists() {
        return Ok(bundled_node);
    }

    let mut probe_command = Command::new("node");
    apply_windows_process_flags(&mut probe_command);
    let probe = probe_command
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match probe {
        Ok(status) if status.success() => Ok(PathBuf::from("node")),
        _ => Err("未找到 Node.js，无法启用真流式聊天。".into()),
    }
}

fn resolve_resource_script_path(app: &AppHandle, file_name: &str) -> Result<PathBuf, String> {
    let packaged = app
        .path()
        .resource_dir()
        .map_err(to_string_error)?
        .join("resources")
        .join(file_name);
    if packaged.exists() {
        return Ok(packaged);
    }

    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(file_name);
    if dev_path.exists() {
        return Ok(dev_path);
    }

    Err(format!("未找到资源脚本：{file_name}"))
}

fn resolve_acp_helper_path(app: &AppHandle) -> Result<PathBuf, String> {
    resolve_resource_script_path(app, "acp_stream_helper.mjs")
}

fn resolve_gateway_subscriber_helper_path(app: &AppHandle) -> Result<PathBuf, String> {
    resolve_resource_script_path(app, "gateway_subscriber.mjs")
}

fn write_acp_payload_file(app: &AppHandle, payload: &AcpStreamPayload) -> Result<PathBuf, String> {
    let dir = app_data_dir(app)?.join("acp");
    fs::create_dir_all(&dir).map_err(to_string_error)?;
    let path = dir.join(format!("{}.json", Uuid::new_v4()));
    write_json(&path, payload)?;
    Ok(path)
}

fn write_gateway_subscriber_payload_file(
    app: &AppHandle,
    payload: &GatewaySubscriberPayload,
) -> Result<PathBuf, String> {
    let dir = app_data_dir(app)?.join("gateway-subscriber");
    fs::create_dir_all(&dir).map_err(to_string_error)?;
    let path = dir.join(format!("{}.json", Uuid::new_v4()));
    write_json(&path, payload)?;
    Ok(path)
}

fn proactive_conversation_id(profile_id: &str, session_key: &str) -> String {
    format!(
        "{}--conv--push-{}",
        profile_session_key(profile_id),
        sanitize_session_key_part(session_key)
    )
}

fn profile_session_key(profile_id: &str) -> String {
    if profile_id == LOCAL_PROFILE_ID {
        "local".to_string()
    } else {
        profile_id.to_string()
    }
}

fn proactive_conversation_title(session_key: &str) -> String {
    if session_key.ends_with(":main") {
        "龙虾主动消息".to_string()
    } else {
        format!(
            "龙虾主动消息 · {}",
            session_key.rsplit(':').next().unwrap_or(session_key)
        )
    }
}

fn conversation_belongs_to_profile(conversation_id: &str, profile_id: &str) -> bool {
    let prefix = format!("{}--conv--", profile_session_key(profile_id));
    conversation_id.starts_with(&prefix)
}

fn remove_profile_conversations(app: &AppHandle, profile_id: &str) -> Result<(), String> {
    let root = conversations_root(app)?;
    if !root.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(&root).map_err(to_string_error)? {
        let entry = entry.map_err(to_string_error)?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if conversation_belongs_to_profile(stem, profile_id) {
            fs::remove_file(path).map_err(to_string_error)?;
        }
    }

    Ok(())
}

fn append_proactive_message(
    app: &AppHandle,
    profile_id: &str,
    session_key: &str,
    content: &str,
    timestamp: &str,
) -> Result<(), String> {
    let conversation_id = proactive_conversation_id(profile_id, session_key);
    let path = conversation_path(app, &conversation_id)?;
    let mut conversation = if path.exists() {
        read_json::<Conversation>(&path)?
    } else {
        Conversation {
            id: conversation_id.clone(),
            title: proactive_conversation_title(session_key),
            created_at: timestamp.to_string(),
            updated_at: timestamp.to_string(),
            messages: Vec::new(),
        }
    };

    if conversation.messages.iter().any(|message| {
        message.role == "assistant" && message.content == content && message.created_at == timestamp
    }) {
        return Ok(());
    }

    conversation.messages.push(ChatMessage {
        id: Uuid::new_v4().to_string(),
        role: "assistant".into(),
        content: content.to_string(),
        created_at: timestamp.to_string(),
    });
    conversation.updated_at = timestamp.to_string();
    sort_conversation_messages(&mut conversation);
    write_json(&path, &conversation)?;
    emit_conversation_update(app, &conversation)?;
    Ok(())
}

fn gateway_port_is_open(config: &GatewayConfig) -> bool {
    let trimmed = config.url.trim();
    let Some(without_scheme) = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .or_else(|| trimmed.strip_prefix("ws://"))
        .or_else(|| trimmed.strip_prefix("wss://"))
    else {
        return false;
    };

    let host_port = without_scheme.split('/').next().unwrap_or(without_scheme);
    let Ok(addrs) = host_port.to_socket_addrs() else {
        return false;
    };

    addrs
        .into_iter()
        .any(|addr| TcpStream::connect_timeout(&addr, Duration::from_millis(600)).is_ok())
}

fn ensure_gateway_subscriber(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let executable = match settings.openclaw_executable_path.as_ref() {
        Some(value) => PathBuf::from(value),
        None => return Ok(()),
    };
    if !executable.exists() {
        return Ok(());
    }

    let profile_id = settings
        .recent_profile_id
        .clone()
        .unwrap_or_else(|| LOCAL_PROFILE_ID.to_string());
    let launch_target = resolve_launch_target(app, settings, &profile_id)?;
    let gateway_config = gateway_config_for_target(&executable, &launch_target)?;
    let ws_module_path = match resolve_ws_module_path(&executable) {
        Some(path) => path,
        None => return Ok(()),
    };
    let node_path = resolve_node_executable(&executable)?;
    let helper_path = resolve_gateway_subscriber_helper_path(app)?;
    let subscriber_state = app.state::<RuntimeState>();

    {
        let mut runtime = subscriber_state
            .subscriber
            .lock()
            .map_err(|_| "无法读取主动消息订阅状态。".to_string())?;
        let same_profile = runtime.profile_id.as_deref() == Some(&launch_target.profile_id);
        let same_url = runtime.gateway_url.as_deref() == Some(&gateway_config.url);
        let alive = runtime
            .child
            .as_mut()
            .map(|child| matches!(child.try_wait(), Ok(None)))
            .unwrap_or(false);
        if same_profile && same_url && alive {
            return Ok(());
        }
        if let Some(child) = runtime.child.as_mut() {
            let _ = child.kill();
        }
        runtime.child = None;
        runtime.profile_id = None;
        runtime.gateway_url = None;
    }

    let payload = GatewaySubscriberPayload {
        gateway_url: to_gateway_ws_url(&gateway_config.url),
        gateway_token: read_profile_gateway_secret(&launch_target.profile_path, "token"),
        gateway_password: read_profile_gateway_secret(&launch_target.profile_path, "password"),
        ws_module_path: ws_module_path.display().to_string(),
        client_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let payload_path = write_gateway_subscriber_payload_file(app, &payload)?;
    let mut command = Command::new(node_path);
    apply_windows_process_flags(&mut command);
    command
        .arg(helper_path)
        .arg(&payload_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(
            executable
                .parent()
                .map(PathBuf::from)
                .ok_or_else(|| "无法确定 OpenClaw 的启动目录。".to_string())?,
        );
    let mut child = command.spawn().map_err(to_string_error)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法读取主动消息订阅输出。".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "无法读取主动消息订阅错误输出。".to_string())?;
    let app_handle = app.clone();
    let active_profile_id = launch_target.profile_id.clone();
    let payload_cleanup = payload_path.clone();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            let parsed = serde_json::from_str::<GatewaySubscriberEvent>(&line);
            match parsed {
                Ok(GatewaySubscriberEvent::Message {
                    session_key,
                    text,
                    timestamp,
                }) => {
                    let _ = append_proactive_message(
                        &app_handle,
                        &active_profile_id,
                        &session_key,
                        &text,
                        &timestamp,
                    );
                }
                Ok(GatewaySubscriberEvent::Error { error }) => {
                    let _ = app_handle.emit(
                        "chat://error",
                        ChatErrorEvent {
                            conversation_id: proactive_conversation_id(&active_profile_id, "push"),
                            error,
                        },
                    );
                }
                Ok(GatewaySubscriberEvent::Status { .. }) => {}
                Err(_) => {}
            }
        }
        let _ = fs::remove_file(payload_cleanup);
    });
    thread::spawn(move || {
        let mut text = String::new();
        let mut reader = BufReader::new(stderr);
        let _ = reader.read_to_string(&mut text);
    });

    let mut runtime = subscriber_state
        .subscriber
        .lock()
        .map_err(|_| "无法更新主动消息订阅状态。".to_string())?;
    runtime.profile_id = Some(launch_target.profile_id);
    runtime.gateway_url = Some(gateway_config.url);
    runtime.child = Some(child);
    Ok(())
}

#[derive(Clone)]
struct LaunchTarget {
    profile_id: String,
    profile_name: String,
    profile_path: String,
    cli_profile_name: Option<String>,
    use_state_dir_env: bool,
    managed_profile: Option<ManagedProfile>,
}

fn resolve_launch_target(
    app: &AppHandle,
    settings: &AppSettings,
    profile_id: &str,
) -> Result<LaunchTarget, String> {
    let default_data_dir = default_openclaw_data_dir_path();

    if profile_id.is_empty() || profile_id == LOCAL_PROFILE_ID {
        let path = settings
            .openclaw_data_dir
            .clone()
            .or_else(|| {
                default_data_dir
                    .as_ref()
                    .map(|path| path.display().to_string())
            })
            .ok_or_else(|| {
                "没有找到默认资料目录，请先确认 C:\\Users\\用户名\\.openclaw 是否存在。".to_string()
            })?;

        if !looks_like_openclaw_data_dir(Path::new(&path)) {
            return Err(
                "默认资料目录无效，请先确认 C:\\Users\\用户名\\.openclaw 是否完整。".into(),
            );
        }

        let use_state_dir_env = default_data_dir
            .as_ref()
            .map(|default_path| default_path != Path::new(&path))
            .unwrap_or(true);

        return Ok(LaunchTarget {
            profile_id: LOCAL_PROFILE_ID.to_string(),
            profile_name: "默认本地龙虾".to_string(),
            profile_path: path,
            cli_profile_name: None,
            use_state_dir_env,
            managed_profile: None,
        });
    }

    let profile = list_profiles_impl(app.clone())?
        .into_iter()
        .find(|item| item.id == profile_id)
        .ok_or_else(|| "没有找到要启动的资料.".to_string())?;
    let profile = ensure_managed_profile_launch_path(&app, settings, profile)?;
    if !Path::new(&profile.path).is_dir() {
        return Err("选中的资料目录不存在，请重新导入。".into());
    }
    Ok(LaunchTarget {
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        profile_path: profile.path.clone(),
        cli_profile_name: Some(cli_profile_name_for(&profile.name, &profile.id)),
        use_state_dir_env: false,
        managed_profile: Some(profile),
    })
}

fn resolve_profile_root(
    app: &AppHandle,
    settings: &AppSettings,
    profile_id: &str,
) -> Result<PathBuf, String> {
    if profile_id.is_empty() || profile_id == LOCAL_PROFILE_ID {
        let path = settings
            .openclaw_data_dir
            .clone()
            .or_else(|| default_openclaw_data_dir_path().map(|path| display(&path)))
            .ok_or_else(|| "没有找到默认本地龙虾目录.".to_string())?;
        return Ok(PathBuf::from(path));
    }

    let profile = list_profiles_impl(app.clone())?
        .into_iter()
        .find(|item| item.id == profile_id)
        .ok_or_else(|| "没有找到这只龙虾.".to_string())?;
    Ok(PathBuf::from(profile.path))
}

fn is_managed_profile_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.starts_with(".openclaw-") || value.starts_with("openclaw-"))
}

fn managed_profile_name_from_dir(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(|value| {
            value
                .trim_start_matches(".openclaw-")
                .trim_start_matches("openclaw-")
                .to_string()
        })
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "profile".to_string())
}

fn profile_cli_name_from_path(path: &Path) -> Option<String> {
    let user_home = default_user_home()?;
    if path.parent()? != user_home {
        return None;
    }
    let name = path.file_name()?.to_str()?;
    let cli_name = name.strip_prefix(".openclaw-")?;
    is_valid_cli_profile_name(cli_name).then(|| cli_name.to_string())
}

fn ensure_managed_profile_launch_path(
    app: &AppHandle,
    settings: &AppSettings,
    profile: ManagedProfile,
) -> Result<ManagedProfile, String> {
    let current_path = PathBuf::from(&profile.path);
    if !current_path.is_dir() {
        return Ok(profile);
    }

    let root = profiles_root(app, settings)?;
    let desired_cli_name = cli_profile_name_for(&profile.name, &profile.id);
    let desired_dir_name = format!(".openclaw-{desired_cli_name}");
    let desired_path = root.join(&desired_dir_name);
    if current_path == desired_path {
        normalize_managed_profile_runtime(&desired_path, &profile.id)?;
        return Ok(profile);
    }

    if desired_path.exists() {
        return Err("这只龙虾的内部启动目录已存在，请先删除重复目录后重试。".to_string());
    }

    if fs::rename(&current_path, &desired_path).is_err() {
        copy_dir_recursive(&current_path, &desired_path)?;
        fs::remove_dir_all(&current_path).map_err(to_string_error)?;
    }

    normalize_managed_profile_runtime(&desired_path, &profile.id)?;

    Ok(ManagedProfile {
        path: desired_path.display().to_string(),
        ..profile
    })
}

#[allow(unreachable_code)]
fn load_profile_metadata(path: &Path) -> Result<ManagedProfile, String> {
    load_profile_metadata_impl(path)
}

fn unique_profile_dir_name(root: &Path, requested: &str) -> String {
    let base = if requested.starts_with(".openclaw-") {
        requested.to_string()
    } else {
        format!(".openclaw-{requested}")
    };
    if !root.join(&base).exists() {
        return base;
    }
    for suffix in 2..1000 {
        let candidate = format!("{base}-{suffix}");
        if !root.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{base}-{}", Uuid::new_v4())
}

fn sanitize_name(value: &str) -> String {
    let filtered: String = value
        .chars()
        .filter(|character| {
            !matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            )
        })
        .map(|character| {
            if character.is_whitespace() {
                '-'
            } else {
                character
            }
        })
        .collect();
    if filtered.trim_matches('-').is_empty() {
        "profile".into()
    } else {
        filtered.trim_matches('-').to_string()
    }
}

fn cli_profile_name_for(display_name: &str, profile_id: &str) -> String {
    let _ = display_name;
    format!("profile-{}", profile_id.replace('-', ""))
}

fn normalize_managed_profile_runtime(profile_root: &Path, profile_id: &str) -> Result<(), String> {
    fs::create_dir_all(profile_root.join("workspace")).map_err(to_string_error)?;

    let config_path = profile_root.join("openclaw.json");
    if config_path.is_file() {
        let mut config = read_json::<serde_json::Value>(&config_path)?;
        let workspace = profile_root.join("workspace").display().to_string();
        let gateway_port = managed_gateway_port(profile_id);
        let gateway_token = format!("launcher-{}", profile_id.replace('-', ""));

        let object = config
            .as_object_mut()
            .ok_or_else(|| "openclaw.json 格式无效。".to_string())?;

        let agents = object
            .entry("agents")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| "openclaw.json 中的 agents 配置无效。".to_string())?;
        let defaults = agents
            .entry("defaults")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| "openclaw.json 中的 agents.defaults 配置无效。".to_string())?;
        defaults.insert("workspace".into(), serde_json::Value::String(workspace));

        let gateway = object
            .entry("gateway")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| "openclaw.json 中的 gateway 配置无效。".to_string())?;
        gateway.insert(
            "port".into(),
            serde_json::Value::Number(gateway_port.into()),
        );
        let auth = gateway
            .entry("auth")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| "openclaw.json 中的 gateway.auth 配置无效。".to_string())?;
        auth.insert("mode".into(), serde_json::Value::String("token".into()));
        auth.insert("token".into(), serde_json::Value::String(gateway_token));
        let remote = gateway
            .entry("remote")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| "openclaw.json 中的 gateway.remote 配置无效。".to_string())?;
        remote.remove("mode");
        remote.insert(
            "url".into(),
            serde_json::Value::String(format!("ws://127.0.0.1:{gateway_port}")),
        );
        remote.insert(
            "token".into(),
            serde_json::Value::String(format!("launcher-{}", profile_id.replace('-', ""))),
        );

        write_json(&config_path, &config)?;
    }

    bootstrap_managed_auth(profile_root)?;
    Ok(())
}

fn bootstrap_managed_auth(profile_root: &Path) -> Result<(), String> {
    let target = profile_root
        .join("agents")
        .join("main")
        .join("agent")
        .join("auth-profiles.json");
    if target.is_file() {
        return Ok(());
    }

    let Some(default_root) = default_openclaw_data_dir_path() else {
        return Ok(());
    };
    let source = default_root
        .join("agents")
        .join("main")
        .join("agent")
        .join("auth-profiles.json");
    if !source.is_file() {
        return Ok(());
    }

    ensure_parent(&target)?;
    fs::copy(source, target).map_err(to_string_error)?;
    Ok(())
}

fn managed_gateway_port(profile_id: &str) -> u16 {
    let mut hash = 0u32;
    for byte in profile_id.bytes() {
        if byte == b'-' {
            continue;
        }
        hash = hash.wrapping_mul(131).wrapping_add(byte as u32);
    }
    20000 + (hash % 20000) as u16
}

fn is_valid_cli_profile_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn default_user_home() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        env::var("USERPROFILE").ok().map(PathBuf::from)
    } else {
        env::var("HOME").ok().map(PathBuf::from)
    }
}

fn default_desktop_path() -> Option<PathBuf> {
    default_user_home().map(|home| {
        let desktop = home.join("Desktop");
        if desktop.is_dir() {
            desktop
        } else {
            home
        }
    })
}

fn process_launch_file_args(app: AppHandle) -> Result<(), String> {
    let launch_files: Vec<PathBuf> = env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter(|path| path.is_file() && has_claw_extension(path))
        .collect();

    for path in launch_files {
        let _ = import_profile_impl(&app, path, None, false);
    }

    Ok(())
}

fn default_openclaw_data_dir_path() -> Option<PathBuf> {
    default_user_home()
        .map(|path| path.join(".openclaw"))
        .filter(|path| looks_like_openclaw_data_dir(path))
}

fn looks_like_openclaw_data_dir(path: &Path) -> bool {
    path.is_dir()
        && (path.join("openclaw.json").is_file()
            || path.join("agents").is_dir()
            || path.join("workspace").is_dir()
            || path.join("credentials").is_dir()
            || path.join("hooks").is_dir())
}

fn detect_first_openclaw_command() -> Option<PathBuf> {
    let mut seen = HashSet::new();
    for (candidate, _) in common_openclaw_candidates() {
        let path = PathBuf::from(candidate);
        if seen.insert(path.clone()) && is_valid_openclaw_command_path(&path) {
            return Some(path);
        }
    }
    None
}

fn is_valid_openclaw_command_path(path: &Path) -> bool {
    if !path.exists() || !path.is_file() {
        return false;
    }

    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let file_name = file_name.to_ascii_lowercase();
    if matches!(
        file_name.as_str(),
        "gateway" | "gateway.cmd" | "gateway.bat" | "gateway.exe"
    ) {
        return false;
    }

    file_name.starts_with("openclaw")
}

fn where_openclaw_candidates() -> Vec<PathBuf> {
    if !cfg!(target_os = "windows") {
        return Vec::new();
    }

    let Ok(output) = Command::new("where").arg("openclaw").output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn is_cmd_like(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("cmd") || value.eq_ignore_ascii_case("bat"))
}

fn normalize_openclaw_command_path(path: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    if let Some(app_binary) = resolve_macos_app_binary(path) {
        return Some(app_binary);
    }

    if path.extension().is_some() {
        return path.exists().then(|| path.to_path_buf());
    }

    if path.exists() && path.is_file() {
        return Some(path.to_path_buf());
    }

    if cfg!(target_os = "windows") {
        for extension in ["cmd", "bat", "exe"] {
            let candidate = path.with_extension(extension);
            if candidate.exists() && candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn resolve_macos_app_binary(path: &Path) -> Option<PathBuf> {
    let bundle_path = if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("app"))
        && path.is_dir()
    {
        Some(path.to_path_buf())
    } else {
        path.components()
            .position(|component| component.as_os_str().to_string_lossy().ends_with(".app"))
            .map(|index| path.components().take(index + 1).collect::<PathBuf>())
            .filter(|bundle| bundle.is_dir())
    }?;

    for candidate in ["OpenClaw", "openclaw"] {
        let binary = bundle_path.join("Contents").join("MacOS").join(candidate);
        if binary.is_file() {
            return Some(binary);
        }
    }

    None
}

fn build_openclaw_command(executable_path: &Path) -> Command {
    if is_cmd_like(executable_path) {
        let mut command = Command::new("cmd");
        command.arg("/C").arg(executable_path);
        apply_windows_process_flags(&mut command);
        command
    } else {
        let mut command = Command::new(executable_path);
        apply_windows_process_flags(&mut command);
        command
    }
}

fn apply_windows_process_flags(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

fn command_supports_profile_switch(executable_path: &Path) -> bool {
    if is_cmd_like(executable_path) {
        return true;
    }

    build_openclaw_command(executable_path)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(true)
}

fn apply_gateway_defaults(settings: &mut AppSettings) {
    if settings
        .gateway_config
        .command
        .as_ref()
        .is_some_and(|value| !PathBuf::from(value).exists())
    {
        settings.gateway_config.command = None;
    }

    if settings.gateway_config.command.is_none() {
        settings.gateway_config.command = settings
            .openclaw_executable_path
            .as_ref()
            .and_then(|value| {
                normalize_openclaw_command_path(Path::new(value))
                    .or_else(|| Some(PathBuf::from(value)))
            })
            .filter(|path| is_valid_openclaw_command_path(path))
            .map(|path| path.display().to_string());
    }

    let Some(data_dir) = settings.openclaw_data_dir.as_ref().map(PathBuf::from) else {
        return;
    };

    let default_url = GatewayConfig::default().url;
    if settings.gateway_config.url == default_url {
        if let Some(url) = read_gateway_remote_url(&data_dir) {
            settings.gateway_config.url = url;
        } else if let Some(port) = read_gateway_port(&data_dir) {
            settings.gateway_config.url = format!("http://127.0.0.1:{port}");
        }
    }
}

fn read_gateway_port(data_dir: &Path) -> Option<u64> {
    let config_path = data_dir.join("openclaw.json");
    let config = read_json::<OpenClawFileConfig>(&config_path).ok()?;
    config.gateway.and_then(|gateway| gateway.port)
}

fn read_gateway_remote_url(data_dir: &Path) -> Option<String> {
    let config_path = data_dir.join("openclaw.json");
    let config = read_json::<OpenClawFileConfig>(&config_path).ok()?;
    let raw = config
        .gateway?
        .remote?
        .url?
        .trim()
        .trim_end_matches('/')
        .to_string();
    if raw.is_empty() {
        return None;
    }
    Some(gateway_http_url_from_remote(&raw))
}

fn gateway_http_url_from_remote(url: &str) -> String {
    if let Some(value) = url.strip_prefix("ws://") {
        format!("http://{value}")
    } else if let Some(value) = url.strip_prefix("wss://") {
        format!("https://{value}")
    } else {
        url.to_string()
    }
}

fn health_check(config: &GatewayConfig) -> Result<(), String> {
    let endpoint = if config.health_endpoint.starts_with('/') {
        config.health_endpoint.clone()
    } else {
        format!("/{}", config.health_endpoint)
    };
    let url = format!("{}{}", config.url.trim_end_matches('/'), endpoint);
    let response = Client::builder()
        .timeout(Duration::from_millis(900))
        .build()
        .map_err(to_string_error)?
        .get(url)
        .send()
        .map_err(to_string_error)?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("健康检查返回了状态码 {}.", response.status()))
    }
}

fn health_check_with_retry(
    config: &GatewayConfig,
    attempts: usize,
    delay: Duration,
) -> Result<(), String> {
    let mut last_error = None;
    for _ in 0..attempts {
        match health_check(config) {
            Ok(_) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
        thread::sleep(delay);
    }
    Err(last_error.unwrap_or_else(|| "健康检查失败.".into()))
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join(SETTINGS_FILE))
}

fn conversations_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join(CONVERSATIONS_DIR))
}

fn conversation_path(app: &AppHandle, conversation_id: &str) -> Result<PathBuf, String> {
    Ok(conversations_root(app)?.join(format!("{conversation_id}.json")))
}

fn conversation_summaries_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(conversations_root(app)?.join("_summaries"))
}

fn conversation_summary_path(app: &AppHandle, conversation_id: &str) -> Result<PathBuf, String> {
    Ok(conversation_summaries_root(app)?.join(format!("{conversation_id}.json")))
}

fn conversation_to_summary(conversation: &Conversation) -> ConversationSummary {
    ConversationSummary {
        id: conversation.id.clone(),
        title: conversation.title.clone(),
        created_at: conversation.created_at.clone(),
        updated_at: conversation.updated_at.clone(),
    }
}

fn write_conversation_summary(app: &AppHandle, conversation: &Conversation) -> Result<(), String> {
    let path = conversation_summary_path(app, &conversation.id)?;
    write_json(&path, &conversation_to_summary(conversation))
}

fn read_or_build_conversation_summary(
    app: &AppHandle,
    conversation_file: &Path,
) -> Result<Option<ConversationSummary>, String> {
    let conversation_id = conversation_file
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "对话文件名无效。".to_string())?;
    let summary_path = conversation_summary_path(app, conversation_id)?;

    if summary_path.exists() {
        let summary_time = fs::metadata(&summary_path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let conversation_time = fs::metadata(conversation_file)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if summary_time >= conversation_time {
            return Ok(Some(read_json::<ConversationSummary>(&summary_path)?));
        }
    }

    if !conversation_file.exists() {
        return Ok(None);
    }

    let conversation = read_json::<Conversation>(conversation_file)?;
    let summary = conversation_to_summary(&conversation);
    write_json(&summary_path, &summary)?;
    Ok(Some(summary))
}

fn default_profiles_root(_app: &AppHandle) -> Result<PathBuf, String> {
    default_user_home().ok_or_else(|| "无法定位当前用户目录.".to_string())
}

fn legacy_profiles_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("profiles"))
}

fn settings_uses_legacy_profiles_root(app: &AppHandle, settings: &AppSettings) -> bool {
    let Some(current_root) = settings.profiles_root.as_ref().map(PathBuf::from) else {
        return false;
    };
    let Ok(legacy_root) = legacy_profiles_root(app) else {
        return false;
    };
    current_root == legacy_root
}

fn migrate_legacy_profiles_if_needed(
    app: &AppHandle,
    settings: &AppSettings,
) -> Result<(), String> {
    let legacy_root = legacy_profiles_root(app)?;
    if !legacy_root.exists() {
        return Ok(());
    }

    let current_root = settings
        .profiles_root
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or(default_profiles_root(app)?);
    fs::create_dir_all(&current_root).map_err(to_string_error)?;

    for entry in fs::read_dir(&legacy_root).map_err(to_string_error)? {
        let entry = entry.map_err(to_string_error)?;
        let path = entry.path();
        if !path.is_dir() || !is_managed_profile_dir(&path) {
            continue;
        }

        let profile = load_profile_metadata_impl(&path)?;
        let cli_name = cli_profile_name_for(&profile.name, &profile.id);
        let target_dir_name = unique_profile_dir_name(&current_root, &cli_name);
        let target_path = current_root.join(target_dir_name);
        if target_path.exists() {
            continue;
        }

        if fs::rename(&path, &target_path).is_err() {
            copy_dir_recursive(&path, &target_path)?;
            fs::remove_dir_all(&path).map_err(to_string_error)?;
        }

        write_json(
            &target_path.join(PROFILE_META_FILE),
            &ProfileMeta {
                id: profile.id,
                name: profile.name,
                imported_from: profile.imported_from,
                created_at: profile.created_at,
                last_used_at: profile.last_used_at,
            },
        )?;
    }

    Ok(())
}

fn profiles_root(app: &AppHandle, settings: &AppSettings) -> Result<PathBuf, String> {
    Ok(settings
        .profiles_root
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or(default_profiles_root(app)?))
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(to_string_error)?;
    for entry in fs::read_dir(source).map_err(to_string_error)? {
        let entry = entry.map_err(to_string_error)?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            ensure_parent(&target_path)?;
            fs::copy(&source_path, &target_path).map_err(to_string_error)?;
        }
    }
    Ok(())
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app.path().app_data_dir().map_err(to_string_error)?;
    fs::create_dir_all(&path).map_err(to_string_error)?;
    Ok(path)
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_string_error)?;
    }
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let contents = fs::read_to_string(path).map_err(to_string_error)?;
    let normalized = contents.trim_start_matches('\u{feff}');
    serde_json::from_str(normalized).map_err(to_string_error)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    ensure_parent(path)?;
    let contents = serde_json::to_string_pretty(value).map_err(to_string_error)?;
    fs::write(path, contents).map_err(to_string_error)
}

fn system_time_to_iso(time: SystemTime) -> String {
    chrono::DateTime::<Utc>::from(time).to_rfc3339()
}

fn format_display_datetime(input: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(input)
        .map(|value| {
            value
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| input.to_string())
}

fn json_value_to_compact_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        serde_json::Value::Bool(value) => Some(value.to_string()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => serde_json::to_string(value).ok(),
    }
}

fn schedule_summary_from_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(_)
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_) => json_value_to_compact_text(value),
        serde_json::Value::Array(items) => {
            let summaries = items
                .iter()
                .filter_map(schedule_summary_from_value)
                .collect::<Vec<_>>();
            if summaries.is_empty() {
                None
            } else {
                Some(summaries.join("；"))
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(at) = map.get("at").and_then(|item| item.as_str()) {
                let prefix = match map.get("kind").and_then(|item| item.as_str()) {
                    Some("cron") => "Cron",
                    Some("every") => "间隔",
                    _ => "单次",
                };
                return Some(format!("{prefix}：{}", format_display_datetime(at)));
            }

            if let Some(expression) = map
                .get("expression")
                .or_else(|| map.get("cron"))
                .or_else(|| map.get("value"))
                .and_then(|item| item.as_str())
            {
                let prefix = match map.get("kind").and_then(|item| item.as_str()) {
                    Some("every") => "间隔",
                    _ => "Cron",
                };
                return Some(format!("{prefix}：{expression}"));
            }

            if let Some(kind) = map.get("kind").and_then(|item| item.as_str()) {
                let trimmed_kind = kind.trim();
                if !trimmed_kind.is_empty() {
                    return Some(format!("计划：{trimmed_kind}"));
                }
            }

            serde_json::to_string(value).ok()
        }
        serde_json::Value::Null => None,
    }
}

fn cron_job_title(job: &serde_json::Value, index: usize) -> String {
    job.get("name")
        .or_else(|| job.get("id"))
        .and_then(|item| item.as_str())
        .map(ToString::to_string)
        .or_else(|| job.get("schedule").and_then(schedule_summary_from_value))
        .or_else(|| job.get("cron").and_then(schedule_summary_from_value))
        .unwrap_or_else(|| format!("任务 {}", index + 1))
}

fn cron_job_subtitle(job: &serde_json::Value) -> String {
    job.get("schedule")
        .and_then(schedule_summary_from_value)
        .or_else(|| job.get("cron").and_then(schedule_summary_from_value))
        .or_else(|| job.get("command").and_then(json_value_to_compact_text))
        .unwrap_or_else(|| "未提供时间信息".into())
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn display(path: &PathBuf) -> String {
    path.display().to_string()
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(to_string_error)?;
    sha256_reader(&mut file)
}

fn sha256_reader<R: Read>(reader: &mut R) -> Result<String, String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = reader.read(&mut buffer).map_err(to_string_error)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn verify_import_package_impl(zip_path: &Path) -> Result<ImportVerification, String> {
    let file = File::open(zip_path).map_err(to_string_error)?;
    let mut archive = ZipArchive::new(file).map_err(to_string_error)?;
    let manifest = read_package_manifest(&mut archive)?;
    let mut issues = Vec::new();
    let mut seen = HashSet::new();

    if manifest.format_version < 2 {
        issues.push("缺少可校验的 manifest 版本信息.".to_string());
    }

    for entry in &manifest.entries {
        let mut zipped = match archive.by_name(&entry.path) {
            Ok(file) => file,
            Err(_) => {
                issues.push(format!("缺少文件: {}", entry.path));
                continue;
            }
        };

        seen.insert(entry.path.clone());
        let actual_size = zipped.size();
        let actual_hash = sha256_reader(&mut zipped)?;
        if actual_size != entry.size {
            issues.push(format!("文件大小不匹配: {}", entry.path));
        }
        if actual_hash != entry.sha256 {
            issues.push(format!("文件哈希不匹配: {}", entry.path));
        }
    }

    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(to_string_error)?;
        let name = file.name().trim_end_matches('/').to_string();
        if name.is_empty() || name == MANIFEST_FILE || file.is_dir() {
            continue;
        }
        if !seen.contains(&name) {
            issues.push(format!("发现 manifest 未记录的文件: {}", name));
        }
    }

    Ok(ImportVerification {
        valid: issues.is_empty(),
        package_name: Some(manifest.package_name),
        exported_at: Some(manifest.exported_at),
        issues,
    })
}

fn read_package_manifest<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<PackageManifest, String> {
    let mut manifest_file = archive
        .by_name(MANIFEST_FILE)
        .map_err(|_| "压缩包缺少 manifest.json.".to_string())?;
    let mut contents = String::new();
    manifest_file
        .read_to_string(&mut contents)
        .map_err(to_string_error)?;
    serde_json::from_str(&contents).map_err(to_string_error)
}

fn tail<T: Clone>(items: Vec<T>, size: usize) -> Vec<T> {
    let len = items.len();
    if len <= size {
        items
    } else {
        items[len - size..].to_vec()
    }
}

fn to_string_error<E: std::fmt::Display>(error: E) -> String {
    error.to_string()
}

fn profile_inventory(root: &Path) -> Result<ProfileInventory, String> {
    Ok(ProfileInventory {
        setting_documents: collect_setting_document_items(root)?,
        skills: collect_skill_items(root)?,
        cron_jobs: collect_cron_items(root)?,
        memories: collect_memory_items(root)?,
        accounts: collect_account_items(root)?,
    })
}

fn preview_profile_item_impl(
    root: &Path,
    section: &str,
    item_id: &str,
) -> Result<ProfileItemPreview, String> {
    match section {
        "settingDocuments" => preview_setting_document_item(root, item_id),
        "skills" => preview_skill_item(root, item_id),
        "cronJobs" => preview_cron_item(root, item_id),
        "memories" => preview_memory_item(root, item_id),
        "accounts" => preview_account_item(root, item_id),
        _ => Err("不支持的预览类型.".to_string()),
    }
}

fn read_profile_readme_impl(root: &Path) -> Result<Option<ProfileItemPreview>, String> {
    for entry in fs::read_dir(root).map_err(to_string_error)? {
        let entry = entry.map_err(to_string_error)?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !name.eq_ignore_ascii_case("README.md") {
            continue;
        }

        let metadata = fs::metadata(&path).map_err(to_string_error)?;
        return Ok(Some(ProfileItemPreview {
            title: "README".into(),
            subtitle: path.display().to_string(),
            content: fs::read_to_string(&path).map_err(to_string_error)?,
            updated_at: metadata.modified().ok().map(system_time_to_iso),
        }));
    }

    Ok(None)
}

fn save_profile_readme_impl(root: &Path, content: &str) -> Result<ProfileItemPreview, String> {
    fs::create_dir_all(root).map_err(to_string_error)?;
    let path = root.join("README.md");
    fs::write(&path, content).map_err(to_string_error)?;
    let metadata = fs::metadata(&path).map_err(to_string_error)?;
    Ok(ProfileItemPreview {
        title: "README".into(),
        subtitle: path.display().to_string(),
        content: content.to_string(),
        updated_at: metadata.modified().ok().map(system_time_to_iso),
    })
}

fn collect_notifications(root: &Path) -> Result<Vec<NotificationItem>, String> {
    let mut items = Vec::new();
    collect_cron_run_notifications(root, &mut items)?;
    collect_heartbeat_notifications(root, &mut items)?;
    items.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.id.cmp(&left.id))
    });
    items.truncate(100);
    Ok(items)
}

fn collect_cron_run_notifications(
    root: &Path,
    output: &mut Vec<NotificationItem>,
) -> Result<(), String> {
    let runs_root = root.join("cron").join("runs");
    if !runs_root.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(runs_root).map_err(to_string_error)? {
        let entry = entry.map_err(to_string_error)?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }
        for line in BufReader::new(File::open(&path).map_err(to_string_error)?)
            .lines()
            .map_while(Result::ok)
        {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if value.get("action").and_then(|value| value.as_str()) != Some("finished") {
                continue;
            }
            let summary = value
                .get("summary")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .unwrap_or_default();
            let error = value
                .get("error")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .unwrap_or_default();
            let content = if !summary.is_empty() && !error.is_empty() {
                format!("{summary}\n\n错误：{error}")
            } else if !summary.is_empty() {
                summary.to_string()
            } else if !error.is_empty() {
                error.to_string()
            } else {
                continue;
            };
            let created_at = value
                .get("ts")
                .and_then(|value| value.as_i64())
                .and_then(timestamp_ms_to_iso)
                .unwrap_or_else(now_iso);
            let status = value
                .get("status")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            let job_id = value
                .get("jobId")
                .and_then(|value| value.as_str())
                .unwrap_or("cron");
            output.push(NotificationItem {
                id: format!("cron-run:{job_id}:{created_at}"),
                title: "定时任务".into(),
                subtitle: format!("状态：{status}"),
                content,
                created_at,
                kind: "cron".into(),
            });
        }
    }

    Ok(())
}

fn collect_heartbeat_notifications(
    root: &Path,
    output: &mut Vec<NotificationItem>,
) -> Result<(), String> {
    let sessions_path = root
        .join("agents")
        .join("main")
        .join("sessions")
        .join("sessions.json");
    if !sessions_path.is_file() {
        return Ok(());
    }

    let sessions = read_json::<serde_json::Value>(&sessions_path)?;
    let Some(entries) = sessions.as_object() else {
        return Ok(());
    };

    for (session_key, entry) in entries {
        if !session_is_heartbeat(entry) {
            continue;
        }
        let Some(session_file) = entry.get("sessionFile").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(mut content) = latest_assistant_text_from_session_file(Path::new(session_file))?
        else {
            continue;
        };
        if content.trim() == "NO_REPLY" {
            content = "Heartbeat 已执行，这次没有生成需要转发给你的新消息。".into();
        }
        let created_at = entry
            .get("updatedAt")
            .and_then(|value| value.as_i64())
            .and_then(timestamp_ms_to_iso)
            .unwrap_or_else(now_iso);
        output.push(NotificationItem {
            id: format!("heartbeat:{session_key}:{created_at}"),
            title: "Heartbeat".into(),
            subtitle: session_key.to_string(),
            content,
            created_at,
            kind: "heartbeat".into(),
        });
    }

    Ok(())
}

fn session_is_heartbeat(entry: &serde_json::Value) -> bool {
    let origin = entry.get("origin").and_then(|value| value.as_object());
    let label = origin
        .and_then(|origin| origin.get("label"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let from = origin
        .and_then(|origin| origin.get("from"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let to = origin
        .and_then(|origin| origin.get("to"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    label.eq_ignore_ascii_case("heartbeat")
        || from.eq_ignore_ascii_case("heartbeat")
        || to.eq_ignore_ascii_case("heartbeat")
}

fn latest_assistant_text_from_session_file(path: &Path) -> Result<Option<String>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let mut latest = None;
    for line in BufReader::new(File::open(path).map_err(to_string_error)?)
        .lines()
        .map_while(Result::ok)
    {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let Some(message) = value.get("message").and_then(|value| value.as_object()) else {
            continue;
        };
        if message.get("role").and_then(|value| value.as_str()) != Some("assistant") {
            continue;
        }
        let Some(content) = message.get("content") else {
            continue;
        };
        let text = transcript_text_content(content);
        if !text.trim().is_empty() {
            latest = Some(text);
        }
    }
    Ok(latest)
}

fn transcript_text_content(content: &serde_json::Value) -> String {
    if let Some(text) = content.as_str() {
        return text.trim().to_string();
    }
    let Some(blocks) = content.as_array() else {
        return String::new();
    };
    let mut parts = Vec::new();
    for block in blocks {
        if block.get("type").and_then(|value| value.as_str()) == Some("text") {
            if let Some(text) = block.get("text").and_then(|value| value.as_str()) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
        }
    }
    parts.join("\n\n")
}

fn timestamp_ms_to_iso(timestamp_ms: i64) -> Option<String> {
    chrono::DateTime::<Utc>::from_timestamp_millis(timestamp_ms).map(|value| value.to_rfc3339())
}

fn preview_setting_document_item(root: &Path, item_id: &str) -> Result<ProfileItemPreview, String> {
    let file_name = safe_item_name(item_id)?;
    if !is_setting_document_name(file_name) {
        return Err("没有找到要预览的设定文档.".to_string());
    }
    let path = root.join("workspace").join(file_name);
    if !path.is_file() {
        return Err("没有找到要预览的设定文档.".to_string());
    }
    let metadata = fs::metadata(&path).map_err(to_string_error)?;
    Ok(ProfileItemPreview {
        title: file_name.to_string(),
        subtitle: "工作区设定文档".into(),
        content: read_text_preview(&path)?,
        updated_at: metadata.modified().ok().map(system_time_to_iso),
    })
}

fn preview_skill_item(root: &Path, item_id: &str) -> Result<ProfileItemPreview, String> {
    let skill_name = safe_item_name(item_id)?;
    let path = skill_directory_candidates(root)
        .into_iter()
        .map(|skills_root| skills_root.join(skill_name).join("SKILL.md"))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| "没有找到要预览的技能文件.".to_string())?;
    let metadata = fs::metadata(&path).map_err(to_string_error)?;
    Ok(ProfileItemPreview {
        title: skill_name.to_string(),
        subtitle: "Workspace Skill".into(),
        content: read_text_preview(&path)?,
        updated_at: metadata.modified().ok().map(system_time_to_iso),
    })
}

fn preview_cron_item(root: &Path, item_id: &str) -> Result<ProfileItemPreview, String> {
    let index = item_id
        .strip_prefix("job-")
        .ok_or_else(|| "没有找到要预览的定时任务.".to_string())?
        .parse::<usize>()
        .map_err(to_string_error)?;
    let path = root.join("cron").join("jobs.json");
    let value = read_json::<serde_json::Value>(&path)?;
    let jobs = value
        .get("jobs")
        .and_then(|item| item.as_array())
        .ok_or_else(|| "没有找到定时任务列表.".to_string())?;
    let job = jobs
        .get(index)
        .ok_or_else(|| "没有找到这条定时任务.".to_string())?;
    let title = cron_job_title(job, index);
    let subtitle = cron_job_subtitle(job);
    Ok(ProfileItemPreview {
        title,
        subtitle,
        content: serde_json::to_string_pretty(&localize_preview_json_times(job))
            .map_err(to_string_error)?,
        updated_at: fs::metadata(&path)
            .map_err(to_string_error)?
            .modified()
            .ok()
            .map(system_time_to_iso),
    })
}

fn preview_memory_item(root: &Path, item_id: &str) -> Result<ProfileItemPreview, String> {
    if item_id == "memory-db" {
        let path = root.join("memory").join("main.sqlite");
        let metadata = fs::metadata(&path).map_err(to_string_error)?;
        return Ok(ProfileItemPreview {
            title: "主记忆库".into(),
            subtitle: format!("main.sqlite · {} 字节", metadata.len()),
            content: format!(
                "这是主记忆数据库文件，当前不直接解析 SQLite 内容。\n\n路径：{}\n大小：{} 字节",
                path.display(),
                metadata.len()
            ),
            updated_at: metadata.modified().ok().map(system_time_to_iso),
        });
    }

    let file_name = safe_item_name(item_id)?;
    let path = root
        .join("agents")
        .join("main")
        .join("sessions")
        .join(file_name);
    if !path.is_file() {
        return Err("没有找到要预览的记忆文件.".to_string());
    }
    let metadata = fs::metadata(&path).map_err(to_string_error)?;
    Ok(ProfileItemPreview {
        title: file_name.to_string(),
        subtitle: format!("会话记忆 · {} 字节", metadata.len()),
        content: read_text_preview(&path)?,
        updated_at: metadata.modified().ok().map(system_time_to_iso),
    })
}

fn preview_account_item(root: &Path, item_id: &str) -> Result<ProfileItemPreview, String> {
    if let Some(key) = item_id.strip_prefix("auth-") {
        let path = root
            .join("agents")
            .join("main")
            .join("agent")
            .join("auth-profiles.json");
        let value = read_json::<serde_json::Value>(&path)?;
        let profile = value
            .get("profiles")
            .and_then(|item| item.as_object())
            .and_then(|profiles| profiles.get(key))
            .ok_or_else(|| "没有找到要预览的账号配置.".to_string())?;
        let provider = profile
            .get("provider")
            .and_then(|item| item.as_str())
            .unwrap_or("未知提供方");
        let auth_type = profile
            .get("type")
            .and_then(|item| item.as_str())
            .unwrap_or("未知类型");
        return Ok(ProfileItemPreview {
            title: key.to_string(),
            subtitle: format!("{provider} · {auth_type}"),
            content: serde_json::to_string_pretty(profile).map_err(to_string_error)?,
            updated_at: fs::metadata(&path)
                .map_err(to_string_error)?
                .modified()
                .ok()
                .map(system_time_to_iso),
        });
    }

    if let Some(device_id) = item_id.strip_prefix("device-") {
        let path = root.join("devices").join("paired.json");
        let value = read_json::<serde_json::Value>(&path)?;
        let device = value
            .as_object()
            .and_then(|devices| devices.get(device_id))
            .ok_or_else(|| "没有找到要预览的设备信息.".to_string())?;
        let client = device
            .get("clientId")
            .and_then(|item| item.as_str())
            .unwrap_or("unknown");
        let platform = device
            .get("platform")
            .and_then(|item| item.as_str())
            .unwrap_or("unknown");
        return Ok(ProfileItemPreview {
            title: format!("设备 {}", short_id(device_id)),
            subtitle: format!("{client} · {platform}"),
            content: serde_json::to_string_pretty(device).map_err(to_string_error)?,
            updated_at: device
                .get("approvedAtMs")
                .and_then(|item| item.as_i64())
                .and_then(timestamp_millis_to_iso),
        });
    }

    let file_name = safe_item_name(item_id)?;
    let path = root.join("identity").join(file_name);
    if !path.is_file() {
        return Err("没有找到要预览的身份文件.".to_string());
    }
    let metadata = fs::metadata(&path).map_err(to_string_error)?;
    Ok(ProfileItemPreview {
        title: file_name.to_string(),
        subtitle: "本地身份文件".into(),
        content: read_text_preview(&path)?,
        updated_at: metadata.modified().ok().map(system_time_to_iso),
    })
}

fn safe_item_name<'a>(value: &'a str) -> Result<&'a str, String> {
    if value.is_empty() || value.contains('/') || value.contains('\\') || value.contains("..") {
        return Err("预览项目名称无效.".to_string());
    }
    Ok(value)
}

fn read_text_preview(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(to_string_error)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut preview = text.chars().take(20_000).collect::<String>();
    if text.chars().count() > 20_000 {
        preview.push_str("\n\n……内容过长，已截断预览。");
    }
    Ok(preview)
}

fn has_claw_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("claw"))
}

fn is_setting_document_name(file_name: &str) -> bool {
    matches!(
        file_name,
        "AGENTS.md"
            | "BOOTSTRAP.md"
            | "HEARTBEAT.md"
            | "IDENTITY.md"
            | "SOUL.md"
            | "TOOLS.md"
            | "USER.md"
    )
}

fn collect_setting_document_items(root: &Path) -> Result<Vec<ProfileListItem>, String> {
    let workspace = root.join("workspace");
    if !workspace.is_dir() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for file_name in [
        "AGENTS.md",
        "BOOTSTRAP.md",
        "HEARTBEAT.md",
        "IDENTITY.md",
        "SOUL.md",
        "TOOLS.md",
        "USER.md",
    ] {
        let path = workspace.join(file_name);
        if !path.is_file() {
            continue;
        }
        let metadata = fs::metadata(&path).map_err(to_string_error)?;
        items.push(ProfileListItem {
            id: file_name.to_string(),
            title: file_name.to_string(),
            subtitle: "工作区设定文档".into(),
            updated_at: metadata.modified().ok().map(system_time_to_iso),
        });
    }
    Ok(items)
}

fn skill_directory_candidates(root: &Path) -> Vec<PathBuf> {
    vec![root.join("workspace").join("skills"), root.join("skills")]
}
fn collect_skill_items(root: &Path) -> Result<Vec<ProfileListItem>, String> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    for skills_root in skill_directory_candidates(root) {
        if !skills_root.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&skills_root).map_err(to_string_error)? {
            let entry = entry.map_err(to_string_error)?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_md_path = path.join("SKILL.md");
            if !skill_md_path.is_file() {
                continue;
            }
            let metadata = fs::metadata(&skill_md_path).map_err(to_string_error)?;
            let skill_name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if !seen.insert(skill_name.clone()) {
                continue;
            }
            items.push(ProfileListItem {
                id: skill_name.clone(),
                title: skill_name,
                subtitle: "Workspace Skill".into(),
                updated_at: metadata.modified().ok().map(system_time_to_iso),
            });
        }
    }
    items.sort_by(|left, right| left.title.cmp(&right.title));
    Ok(items)
}

fn collect_cron_items(root: &Path) -> Result<Vec<ProfileListItem>, String> {
    let path = root.join("cron").join("jobs.json");
    if !path.is_file() {
        return Ok(Vec::new());
    }

    let value = read_json::<serde_json::Value>(&path)?;
    let jobs = value
        .get("jobs")
        .and_then(|item| item.as_array())
        .cloned()
        .unwrap_or_default();

    let mut items = Vec::new();
    for (index, job) in jobs.iter().enumerate() {
        items.push(ProfileListItem {
            id: format!("job-{index}"),
            title: cron_job_title(job, index),
            subtitle: cron_job_subtitle(job),
            updated_at: None,
        });
    }
    Ok(items)
}

fn collect_memory_items(root: &Path) -> Result<Vec<ProfileListItem>, String> {
    let mut items = Vec::new();
    let memory_db = root.join("memory").join("main.sqlite");
    if memory_db.is_file() {
        let metadata = fs::metadata(&memory_db).map_err(to_string_error)?;
        items.push(ProfileListItem {
            id: "memory-db".into(),
            title: "主记忆库".into(),
            subtitle: format!("main.sqlite · {} 字节", metadata.len()),
            updated_at: metadata.modified().ok().map(system_time_to_iso),
        });
    }

    let sessions_dir = root.join("agents").join("main").join("sessions");
    if sessions_dir.is_dir() {
        for entry in fs::read_dir(&sessions_dir).map_err(to_string_error)? {
            let entry = entry.map_err(to_string_error)?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let metadata = fs::metadata(&path).map_err(to_string_error)?;
            items.push(ProfileListItem {
                id: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                title: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                subtitle: format!("会话记忆 · {} 字节", metadata.len()),
                updated_at: metadata.modified().ok().map(system_time_to_iso),
            });
        }
    }

    items.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.title.cmp(&right.title))
    });
    Ok(items)
}

fn collect_account_items(root: &Path) -> Result<Vec<ProfileListItem>, String> {
    let mut items = Vec::new();

    let auth_profiles = root
        .join("agents")
        .join("main")
        .join("agent")
        .join("auth-profiles.json");
    if auth_profiles.is_file() {
        let value = read_json::<serde_json::Value>(&auth_profiles)?;
        if let Some(profiles) = value.get("profiles").and_then(|item| item.as_object()) {
            for (key, profile) in profiles {
                let provider = profile
                    .get("provider")
                    .and_then(|item| item.as_str())
                    .unwrap_or("未知提供方");
                let auth_type = profile
                    .get("type")
                    .and_then(|item| item.as_str())
                    .unwrap_or("未知类型");
                items.push(ProfileListItem {
                    id: format!("auth-{key}"),
                    title: key.to_string(),
                    subtitle: format!("{provider} 路 {auth_type}"),
                    updated_at: None,
                });
            }
        }
    }

    let paired = root.join("devices").join("paired.json");
    if paired.is_file() {
        let value = read_json::<serde_json::Value>(&paired)?;
        if let Some(devices) = value.as_object() {
            for (device_id, device) in devices {
                let client = device
                    .get("clientId")
                    .and_then(|item| item.as_str())
                    .unwrap_or("unknown");
                let platform = device
                    .get("platform")
                    .and_then(|item| item.as_str())
                    .unwrap_or("unknown");
                items.push(ProfileListItem {
                    id: format!("device-{device_id}"),
                    title: format!("设备 {}", short_id(device_id)),
                    subtitle: format!("{client} · {platform}"),
                    updated_at: device
                        .get("approvedAtMs")
                        .and_then(|item| item.as_i64())
                        .and_then(timestamp_millis_to_iso),
                });
            }
        }
    }

    let identity_dir = root.join("identity");
    if identity_dir.is_dir() {
        for file_name in ["device.json", "device-auth.json"] {
            let path = identity_dir.join(file_name);
            if path.is_file() {
                let metadata = fs::metadata(&path).map_err(to_string_error)?;
                items.push(ProfileListItem {
                    id: file_name.to_string(),
                    title: file_name.to_string(),
                    subtitle: "本地身份文件".into(),
                    updated_at: metadata.modified().ok().map(system_time_to_iso),
                });
            }
        }
    }

    items.sort_by(|left, right| left.title.cmp(&right.title));
    Ok(items)
}

fn short_id(value: &str) -> String {
    value.chars().take(8).collect()
}

fn timestamp_millis_to_iso(value: i64) -> Option<String> {
    chrono::DateTime::<Utc>::from_timestamp_millis(value).map(|date| date.to_rfc3339())
}

fn timestamp_millis_to_display(value: i64) -> Option<String> {
    chrono::DateTime::<Utc>::from_timestamp_millis(value).map(|date| {
        date.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    })
}

fn localize_preview_json_times(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(localize_preview_json_times)
                .collect::<Vec<_>>(),
        ),
        serde_json::Value::Object(map) => {
            let localized = map
                .iter()
                .map(|(key, item)| {
                    let next = match item {
                        serde_json::Value::String(text)
                            if key == "at" || key.ends_with("At") || key.ends_with("Time") =>
                        {
                            serde_json::Value::String(format_display_datetime(text))
                        }
                        serde_json::Value::Number(number) if key.ends_with("AtMs") => number
                            .as_i64()
                            .and_then(timestamp_millis_to_display)
                            .map(serde_json::Value::String)
                            .unwrap_or_else(|| item.clone()),
                        _ => localize_preview_json_times(item),
                    };
                    (key.clone(), next)
                })
                .collect::<serde_json::Map<_, _>>();
            serde_json::Value::Object(localized)
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_gateway_defaults, cli_profile_name_for, collect_cron_items,
        collect_setting_document_items, collect_skill_items, export_profile_impl,
        extract_agent_cli_text, infer_data_dir, is_valid_cli_profile_name,
        is_valid_openclaw_command_path, localize_preview_json_times, looks_like_openclaw_data_dir,
        normalize_managed_profile_runtime, preview_cron_item, preview_setting_document_item,
        preview_skill_item, read_json, schedule_summary_from_value, should_skip_export_path,
        verify_import_package_impl, AppSettings, ExportProfileRequest,
    };
    use std::{env, fs, fs::File};
    use uuid::Uuid;
    use walkdir::WalkDir;
    use zip::ZipArchive;

    #[test]
    fn export_defaults_skip_memory_and_account_paths() {
        assert!(should_skip_export_path(
            "agents/main/sessions/item.jsonl",
            false,
            false
        ));
        assert!(should_skip_export_path("devices/paired.json", false, false));
        assert!(should_skip_export_path(
            "identity/device-auth.json",
            false,
            false
        ));
        assert!(should_skip_export_path(
            "agents/main/agent/auth-profiles.json",
            false,
            false
        ));
        assert!(!should_skip_export_path(
            "workspace/project.txt",
            false,
            false
        ));
    }

    #[test]
    fn collect_setting_documents_only_returns_known_workspace_docs() {
        let root = env::temp_dir().join(format!("openclaw-setting-docs-test-{}", Uuid::new_v4()));
        let workspace = root.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        fs::write(workspace.join("AGENTS.md"), "# agents").unwrap();
        fs::write(workspace.join("TOOLS.md"), "# tools").unwrap();
        fs::write(workspace.join("README.md"), "# readme").unwrap();
        fs::write(workspace.join("project.md"), "# project").unwrap();

        let items = collect_setting_document_items(&root).unwrap();
        let ids = items.into_iter().map(|item| item.id).collect::<Vec<_>>();
        assert_eq!(ids, vec!["AGENTS.md".to_string(), "TOOLS.md".to_string()]);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn collect_skills_reads_workspace_skill_directories() {
        let root = env::temp_dir().join(format!("openclaw-skill-dirs-test-{}", Uuid::new_v4()));
        let skills = root.join("workspace").join("skills");
        let root_skills = root.join("skills");
        fs::create_dir_all(skills.join("alpha")).unwrap();
        fs::create_dir_all(skills.join("beta")).unwrap();
        fs::create_dir_all(skills.join("gamma")).unwrap();
        fs::create_dir_all(root_skills.join("delta")).unwrap();
        fs::create_dir_all(root_skills.join("alpha")).unwrap();
        fs::write(skills.join("alpha").join("SKILL.md"), "# alpha").unwrap();
        fs::write(skills.join("beta").join("README.md"), "# beta").unwrap();
        fs::write(root.join("workspace").join("AGENTS.md"), "# agents").unwrap();
        fs::write(skills.join("gamma").join("SKILL.md"), "# gamma").unwrap();
        fs::write(root_skills.join("delta").join("SKILL.md"), "# delta").unwrap();
        fs::write(root_skills.join("alpha").join("SKILL.md"), "# alpha root").unwrap();
        let items = collect_skill_items(&root).unwrap();
        let ids = items.into_iter().map(|item| item.id).collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                "alpha".to_string(),
                "delta".to_string(),
                "gamma".to_string()
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preview_routes_setting_documents_and_skills_to_expected_paths() {
        let root = env::temp_dir().join(format!("openclaw-preview-paths-test-{}", Uuid::new_v4()));
        let workspace = root.join("workspace");
        let skill_dir = workspace.join("skills").join("alpha");
        let root_skill_dir = root.join("skills").join("beta");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::create_dir_all(&root_skill_dir).unwrap();
        fs::write(workspace.join("AGENTS.md"), "# agents").unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# alpha").unwrap();
        fs::write(root_skill_dir.join("SKILL.md"), "# beta").unwrap();
        let setting = preview_setting_document_item(&root, "AGENTS.md").unwrap();
        assert_eq!(setting.title, "AGENTS.md");
        assert!(setting.content.contains("# agents"));
        let skill = preview_skill_item(&root, "alpha").unwrap();
        assert_eq!(skill.title, "alpha");
        assert!(skill.content.contains("# alpha"));
        let root_skill = preview_skill_item(&root, "beta").unwrap();
        assert_eq!(root_skill.title, "beta");
        assert!(root_skill.content.contains("# beta"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn export_can_include_sensitive_paths_when_enabled() {
        assert!(!should_skip_export_path(
            "agents/main/sessions/item.jsonl",
            true,
            false
        ));
        assert!(!should_skip_export_path("devices/paired.json", false, true));
        assert!(!should_skip_export_path(
            "identity/device-auth.json",
            false,
            true
        ));
        assert!(!should_skip_export_path(
            "agents/main/agent/auth-profiles.json",
            false,
            true
        ));
    }

    #[test]
    fn export_profile_excludes_sensitive_files_by_default() {
        let root = env::temp_dir().join(format!("openclaw-export-test-{}", Uuid::new_v4()));
        let source = root.join(".openclaw");
        let zip_path = root.join("safe-export.zip");

        fs::create_dir_all(source.join("agents/main/sessions")).unwrap();
        fs::create_dir_all(source.join("agents/main/agent")).unwrap();
        fs::create_dir_all(source.join("devices")).unwrap();
        fs::create_dir_all(source.join("identity")).unwrap();
        fs::create_dir_all(source.join("workspace")).unwrap();

        fs::write(source.join("openclaw.json"), "{}").unwrap();
        fs::write(source.join("USER.md"), "private user profile").unwrap();
        fs::write(source.join("agents/main/sessions/history.jsonl"), "secret").unwrap();
        fs::write(
            source.join("agents/main/agent/auth-profiles.json"),
            "secret",
        )
        .unwrap();
        fs::write(source.join("devices/paired.json"), "secret").unwrap();
        fs::write(source.join("identity/device-auth.json"), "secret").unwrap();
        fs::write(source.join("workspace/project.txt"), "safe").unwrap();

        let result = export_profile_impl(ExportProfileRequest {
            source_dir: source.display().to_string(),
            zip_path: Some(zip_path.display().to_string()),
            package_name: Some("safe-export".into()),
            include_memory: Some(false),
            include_account_info: Some(false),
        })
        .unwrap();

        let archive_file = File::open(&result.zip_path).unwrap();
        let mut archive = ZipArchive::new(archive_file).unwrap();
        let mut names = Vec::new();
        for index in 0..archive.len() {
            names.push(archive.by_index(index).unwrap().name().to_string());
        }

        assert!(names.contains(&"manifest.json".to_string()));
        assert!(names.contains(&"workspace/project.txt".to_string()));
        assert!(!names
            .iter()
            .any(|name| name.contains("sessions/history.jsonl")));
        assert!(!names.iter().any(|name| name == "USER.md"));
        assert!(!names.iter().any(|name| name.contains("auth-profiles.json")));
        assert!(!names
            .iter()
            .any(|name| name.contains("devices/paired.json")));
        assert!(!names
            .iter()
            .any(|name| name.contains("identity/device-auth.json")));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn looks_like_openclaw_data_dir_accepts_reference_state_layouts() {
        let root = env::temp_dir().join(format!("openclaw-state-layout-test-{}", Uuid::new_v4()));

        fs::create_dir_all(root.join("workspace")).unwrap();
        assert!(looks_like_openclaw_data_dir(&root));

        let _ = fs::remove_dir_all(&root);

        let root = env::temp_dir().join(format!("openclaw-state-layout-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("credentials")).unwrap();
        assert!(looks_like_openclaw_data_dir(&root));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn infer_data_dir_prefers_dot_openclaw_near_binary() {
        let root = env::temp_dir().join(format!("openclaw-infer-data-dir-test-{}", Uuid::new_v4()));
        let bin_dir = root.join("bin");
        let data_dir = root.join(".openclaw");
        let executable = if cfg!(target_os = "windows") {
            bin_dir.join("openclaw.exe")
        } else {
            bin_dir.join("openclaw")
        };

        fs::create_dir_all(&bin_dir).unwrap();
        fs::create_dir_all(data_dir.join("workspace")).unwrap();
        fs::write(&executable, "").unwrap();

        let home_key = if cfg!(target_os = "windows") {
            "USERPROFILE"
        } else {
            "HOME"
        };
        let original_home = env::var_os(home_key);
        env::set_var(home_key, root.join("fake-home"));

        let inferred = infer_data_dir(&executable).unwrap();

        if let Some(value) = original_home {
            env::set_var(home_key, value);
        } else {
            env::remove_var(home_key);
        }

        assert_eq!(inferred, data_dir);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_gateway_defaults_uses_openclaw_executable_instead_of_gateway_cmd() {
        let root =
            env::temp_dir().join(format!("openclaw-gateway-defaults-test-{}", Uuid::new_v4()));
        let data_dir = root.join(".openclaw");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("openclaw.json"), "{}").unwrap();

        let executable = if cfg!(target_os = "windows") {
            root.join("openclaw.exe")
        } else {
            root.join("openclaw")
        };
        fs::write(&executable, "").unwrap();

        let mut settings = AppSettings::default();
        settings.openclaw_executable_path = Some(executable.display().to_string());
        settings.openclaw_data_dir = Some(data_dir.display().to_string());

        apply_gateway_defaults(&mut settings);

        assert_eq!(
            settings.gateway_config.command,
            Some(executable.display().to_string())
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn read_json_accepts_utf8_bom() {
        let root = env::temp_dir().join(format!("openclaw-json-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        fs::write(&path, b"\xEF\xBB\xBF{\"value\":true}").unwrap();

        let value = read_json::<serde_json::Value>(&path).unwrap();
        assert_eq!(
            value.get("value").and_then(|item| item.as_bool()),
            Some(true)
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_import_package_detects_tampering() {
        let root = env::temp_dir().join(format!("openclaw-verify-test-{}", Uuid::new_v4()));
        let source = root.join(".openclaw");
        let zip_path = root.join("verify-export.zip");
        let unpacked = root.join("unzipped");

        fs::create_dir_all(source.join("workspace")).unwrap();
        fs::write(source.join("workspace/project.txt"), "safe").unwrap();

        export_profile_impl(ExportProfileRequest {
            source_dir: source.display().to_string(),
            zip_path: Some(zip_path.display().to_string()),
            package_name: Some("verify-export".into()),
            include_memory: Some(false),
            include_account_info: Some(false),
        })
        .unwrap();

        let valid = verify_import_package_impl(&zip_path).unwrap();
        assert!(valid.valid);

        fs::create_dir_all(&unpacked).unwrap();
        let archive_file = File::open(&zip_path).unwrap();
        let mut archive = ZipArchive::new(archive_file).unwrap();
        archive.extract(&unpacked).unwrap();
        fs::write(unpacked.join("workspace/project.txt"), "changed").unwrap();

        let tampered_zip_path = root.join("verify-export-tampered.zip");
        let tampered_file = File::create(&tampered_zip_path).unwrap();
        let mut tampered_zip = zip::ZipWriter::new(tampered_file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for entry in WalkDir::new(&unpacked) {
            let entry = entry.unwrap();
            let path = entry.path();
            if path == unpacked {
                continue;
            }
            let relative = path
                .strip_prefix(&unpacked)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if entry.file_type().is_dir() {
                tampered_zip.add_directory(relative, options).unwrap();
            } else {
                tampered_zip.start_file(relative, options).unwrap();
                let mut input = File::open(path).unwrap();
                std::io::copy(&mut input, &mut tampered_zip).unwrap();
            }
        }
        tampered_zip.finish().unwrap();

        let tampered = verify_import_package_impl(&tampered_zip_path).unwrap();
        assert!(!tampered.valid);
        assert!(tampered.issues.iter().any(|item| item.contains("哈希")));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn schedule_summary_reads_object_at_time() {
        let value = serde_json::json!({
          "kind": "at",
          "at": "2026-03-10T14:06:16.000Z"
        });

        let summary = schedule_summary_from_value(&value).unwrap();
        assert!(summary.starts_with("单次："));
        assert!(summary.contains("2026-03-10"));
    }

    #[test]
    fn collect_cron_items_reads_schedule_object() {
        let root = env::temp_dir().join(format!("openclaw-cron-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("cron")).unwrap();
        fs::write(
            root.join("cron").join("jobs.json"),
            serde_json::json!({
              "version": 1,
              "jobs": [
                {
                  "id": "job-1",
                  "schedule": {
                    "kind": "at",
                    "at": "2026-03-10T14:06:16.000Z"
                  }
                }
              ]
            })
            .to_string(),
        )
        .unwrap();

        let items = collect_cron_items(&root).unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].subtitle.starts_with("单次："));

        let preview = preview_cron_item(&root, "job-0").unwrap();
        assert!(preview.subtitle.starts_with("单次："));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn extract_agent_cli_text_reads_top_level_payloads() {
        let response = serde_json::json!({
          "payloads": [
            {
              "text": "收到",
              "mediaUrl": null
            }
          ]
        });

        assert_eq!(extract_agent_cli_text(&response), "收到");
    }

    #[test]
    fn localize_preview_json_times_converts_utc_fields() {
        let value = serde_json::json!({
          "schedule": {
            "at": "2026-03-10T14:06:16.000Z"
          },
          "state": {
            "nextRunAtMs": 1773151576000i64
          }
        });

        let localized = localize_preview_json_times(&value);
        assert_ne!(
            localized
                .get("schedule")
                .and_then(|item| item.get("at"))
                .and_then(|item| item.as_str()),
            Some("2026-03-10T14:06:16.000Z")
        );
        assert!(localized
            .get("state")
            .and_then(|item| item.get("nextRunAtMs"))
            .and_then(|item| item.as_str())
            .is_some());
    }

    #[test]
    fn normalize_runtime_removes_invalid_gateway_remote_mode() {
        let root = env::temp_dir().join(format!(
            "openclaw-gateway-normalize-test-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("openclaw.json"),
            serde_json::json!({
              "gateway": {
                "port": 18789,
                "auth": {
                  "mode": "token",
                  "token": "old-token"
                },
                "remote": {
                  "mode": "token",
                  "url": "ws://127.0.0.1:18789",
                  "token": "old-token"
                }
              }
            })
            .to_string(),
        )
        .unwrap();

        normalize_managed_profile_runtime(&root, "12345678-aaaa-bbbb-cccc-abcdef123456").unwrap();
        let config = read_json::<serde_json::Value>(&root.join("openclaw.json")).unwrap();
        assert_eq!(
            config
                .get("gateway")
                .and_then(|item| item.get("remote"))
                .and_then(|item| item.get("mode")),
            None
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn numeric_profile_names_fall_back_to_internal_cli_name() {
        assert!(!is_valid_cli_profile_name("1"));
        assert_eq!(
            cli_profile_name_for("1", "12345678-aaaa-bbbb-cccc-abcdef123456"),
            "profile-12345678aaaabbbbccccabcdef123456"
        );
        assert_eq!(
            cli_profile_name_for("work", "12345678-aaaa-bbbb-cccc-abcdef123456"),
            "profile-12345678aaaabbbbccccabcdef123456"
        );
    }

    #[test]
    fn gateway_binary_is_not_treated_as_openclaw_command() {
        let root = env::temp_dir().join(format!("openclaw-gateway-path-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let gateway_path = root.join(if cfg!(target_os = "windows") {
            "gateway.exe"
        } else {
            "gateway"
        });
        fs::write(&gateway_path, b"").unwrap();

        assert!(!is_valid_openclaw_command_path(&gateway_path));

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn normalize_macos_app_bundle_to_binary() {
        let root = env::temp_dir().join(format!("openclaw-app-path-test-{}", Uuid::new_v4()));
        let app_root = root.join("OpenClaw.app");
        let binary = app_root.join("Contents").join("MacOS").join("OpenClaw");
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::write(&binary, b"#!/bin/sh\n").unwrap();

        assert_eq!(
            normalize_openclaw_command_path(&app_root),
            Some(binary.clone())
        );
        assert_eq!(
            normalize_openclaw_command_path(&binary),
            Some(binary.clone())
        );

        let _ = fs::remove_dir_all(&root);
    }
}
