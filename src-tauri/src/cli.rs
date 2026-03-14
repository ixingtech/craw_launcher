use crate::*;
use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "claws-cli")]
#[command(about = "Standalone CLI for OpenClaw Launcher")]
pub struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    launcher_home: Option<PathBuf>,
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    #[command(subcommand)]
    Settings(SettingsCommand),
    #[command(subcommand)]
    Profiles(ProfileCommand),
    #[command(subcommand)]
    Inventory(InventoryCommand),
    #[command(subcommand)]
    Chat(ChatCommand),
}

#[derive(Subcommand)]
enum SettingsCommand {
    Show,
}

#[derive(Subcommand)]
enum ProfileCommand {
    List,
    Launch(ProfileLaunchArgs),
    Import(ProfileImportArgs),
    Export(ProfileExportArgs),
}

#[derive(Args)]
struct ProfileLaunchArgs {
    profile_id: Option<String>,
}

#[derive(Args)]
struct ProfileImportArgs {
    zip_path: PathBuf,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    ignore_verification: bool,
}

#[derive(Args)]
struct ProfileExportArgs {
    profile_id: String,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long)]
    package_name: Option<String>,
    #[arg(long)]
    include_memory: bool,
    #[arg(long)]
    include_account_info: bool,
}

#[derive(Subcommand)]
enum InventoryCommand {
    Show(InventoryShowArgs),
    Preview(InventoryPreviewArgs),
    Readme(InventoryReadmeArgs),
}

#[derive(Args)]
struct InventoryShowArgs {
    profile_id: String,
}

#[derive(Args)]
struct InventoryPreviewArgs {
    profile_id: String,
    section: String,
    item_id: String,
}

#[derive(Args)]
struct InventoryReadmeArgs {
    profile_id: String,
}

#[derive(Subcommand)]
enum ChatCommand {
    Send(ChatSendArgs),
}

#[derive(Args)]
struct ChatSendArgs {
    message: String,
    #[arg(long)]
    profile_id: Option<String>,
    #[arg(long)]
    conversation_id: Option<String>,
}

#[derive(Serialize)]
struct CliChatResult {
    profile_id: String,
    conversation_id: String,
    content: String,
}

pub fn run() -> Result<(), String> {
    let cli = Cli::parse();
    let context = cli_context(&cli)?;

    match cli.command {
        CliCommand::Settings(SettingsCommand::Show) => {
            let settings = load_settings_in_cli_context(&context)?;
            print_value(&settings, cli.json)
        }
        CliCommand::Profiles(command) => match command {
            ProfileCommand::List => {
                let profiles = list_profiles_in_cli_context(&context)?;
                print_value(&profiles, cli.json)
            }
            ProfileCommand::Launch(args) => {
                let launched = launch_profile_in_cli(&context, args.profile_id)?;
                print_value(&launched, cli.json)
            }
            ProfileCommand::Import(args) => {
                let profile = import_profile_in_cli_context(
                    &context,
                    args.zip_path,
                    args.name,
                    args.ignore_verification,
                )?;
                print_value(&profile, cli.json)
            }
            ProfileCommand::Export(args) => {
                let settings = load_settings_in_cli_context(&context)?;
                let source_dir =
                    resolve_profile_root_in_cli_context(&context, &settings, &args.profile_id)?;
                let package = export_profile_impl(ExportProfileRequest {
                    source_dir: source_dir.display().to_string(),
                    zip_path: args.out.map(|path| path.display().to_string()),
                    package_name: args.package_name,
                    include_memory: Some(args.include_memory),
                    include_account_info: Some(args.include_account_info),
                })?;
                print_value(&package, cli.json)
            }
        },
        CliCommand::Inventory(command) => match command {
            InventoryCommand::Show(args) => {
                let settings = load_settings_in_cli_context(&context)?;
                let root =
                    resolve_profile_root_in_cli_context(&context, &settings, &args.profile_id)?;
                let inventory = profile_inventory(&root)?;
                print_value(&inventory, cli.json)
            }
            InventoryCommand::Preview(args) => {
                let settings = load_settings_in_cli_context(&context)?;
                let root =
                    resolve_profile_root_in_cli_context(&context, &settings, &args.profile_id)?;
                let preview = preview_profile_item_impl(&root, &args.section, &args.item_id)?;
                print_preview(&preview, cli.json)
            }
            InventoryCommand::Readme(args) => {
                let settings = load_settings_in_cli_context(&context)?;
                let root =
                    resolve_profile_root_in_cli_context(&context, &settings, &args.profile_id)?;
                let readme = read_profile_readme_impl(&root)?;
                print_value(&readme, cli.json)
            }
        },
        CliCommand::Chat(ChatCommand::Send(args)) => {
            let result = send_chat_message_in_cli(&context, args)?;
            if cli.json {
                print_value(&result, true)
            } else {
                println!("{}", result.content);
                Ok(())
            }
        }
    }
}

fn cli_context(cli: &Cli) -> Result<LauncherContext, String> {
    if let Some(path) = cli.launcher_home.as_ref() {
        fs::create_dir_all(path).map_err(to_string_error)?;
        return Ok(LauncherContext {
            app_data_dir: path.clone(),
        });
    }

    if let Ok(value) = std::env::var("CLAWS_LAUNCHER_HOME") {
        let path = PathBuf::from(value);
        fs::create_dir_all(&path).map_err(to_string_error)?;
        return Ok(LauncherContext { app_data_dir: path });
    }

    let base = dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .ok_or_else(|| "无法定位启动器数据目录。".to_string())?;
    let path = base.join("claws-launcher");
    fs::create_dir_all(&path).map_err(to_string_error)?;
    Ok(LauncherContext { app_data_dir: path })
}

fn print_value<T: Serialize>(value: &T, _json: bool) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(to_string_error)?
    );
    Ok(())
}

fn print_preview(preview: &ProfileItemPreview, json: bool) -> Result<(), String> {
    if json {
        return print_value(preview, true);
    }
    println!("{}", preview.title);
    if !preview.subtitle.trim().is_empty() {
        println!("{}", preview.subtitle);
    }
    println!();
    println!("{}", preview.content);
    Ok(())
}

fn cli_settings_path(context: &LauncherContext) -> PathBuf {
    context.app_data_dir.join(SETTINGS_FILE)
}

fn cli_conversations_root(context: &LauncherContext) -> PathBuf {
    context.app_data_dir.join(CONVERSATIONS_DIR)
}

fn cli_default_profiles_root() -> Result<PathBuf, String> {
    default_user_home().ok_or_else(|| "无法定位当前用户目录。".to_string())
}

fn cli_legacy_profiles_root(context: &LauncherContext) -> PathBuf {
    context.app_data_dir.join("profiles")
}

fn load_settings_in_cli_context(context: &LauncherContext) -> Result<AppSettings, String> {
    let path = cli_settings_path(context);
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

    let legacy_root = cli_legacy_profiles_root(context);
    let uses_legacy = settings
        .profiles_root
        .as_ref()
        .map(PathBuf::from)
        .is_some_and(|path| path == legacy_root);
    if settings.profiles_root.is_none() || uses_legacy {
        settings.profiles_root = Some(cli_default_profiles_root()?.display().to_string());
    }

    migrate_legacy_profiles_if_needed_in_cli_context(context, &settings)?;
    apply_gateway_defaults(&mut settings);
    if normalized || !settings_file_exists {
        write_json(&path, &settings)?;
    }
    Ok(settings)
}

fn save_settings_in_cli_context(
    context: &LauncherContext,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let path = cli_settings_path(context);
    let mut normalized_settings = settings;
    normalized_settings.openclaw_executable_path = normalized_settings
        .openclaw_executable_path
        .as_ref()
        .and_then(|value| {
            normalize_openclaw_command_path(Path::new(value)).map(|path| display(&path))
        });
    write_json(&path, &normalized_settings)?;
    Ok(normalized_settings)
}

fn migrate_legacy_profiles_if_needed_in_cli_context(
    context: &LauncherContext,
    settings: &AppSettings,
) -> Result<(), String> {
    let legacy_root = cli_legacy_profiles_root(context);
    if !legacy_root.exists() {
        return Ok(());
    }

    let current_root = settings
        .profiles_root
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or(cli_default_profiles_root()?);
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

fn profiles_root_in_cli_context(
    settings: &AppSettings,
) -> Result<PathBuf, String> {
    Ok(settings
        .profiles_root
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or(cli_default_profiles_root()?))
}

fn list_profiles_in_cli_context(context: &LauncherContext) -> Result<Vec<ManagedProfile>, String> {
    let settings = load_settings_in_cli_context(context)?;
    let root = profiles_root_in_cli_context(&settings)?;
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

fn resolve_profile_root_in_cli_context(
    context: &LauncherContext,
    settings: &AppSettings,
    profile_id: &str,
) -> Result<PathBuf, String> {
    if profile_id.is_empty() || profile_id == LOCAL_PROFILE_ID {
        let path = settings
            .openclaw_data_dir
            .clone()
            .or_else(|| default_openclaw_data_dir_path().map(|path| display(&path)))
            .ok_or_else(|| "没有找到默认龙虾目录.".to_string())?;
        return Ok(PathBuf::from(path));
    }

    let profile = list_profiles_in_cli_context(context)?
        .into_iter()
        .find(|item| item.id == profile_id)
        .ok_or_else(|| "没有找到这只龙虾.".to_string())?;
    Ok(PathBuf::from(profile.path))
}

fn ensure_managed_profile_launch_path_in_cli_context(
    settings: &AppSettings,
    profile: ManagedProfile,
) -> Result<ManagedProfile, String> {
    let current_path = PathBuf::from(&profile.path);
    if !current_path.is_dir() {
        return Ok(profile);
    }

    let root = profiles_root_in_cli_context(settings)?;
    let desired_cli_name = cli_profile_name_for(&profile.name, &profile.id);
    let desired_dir_name = format!(".openclaw-{desired_cli_name}");
    let desired_path = root.join(&desired_dir_name);
    if current_path == desired_path {
        normalize_managed_profile_runtime(&desired_path, &profile.id, Some(settings))?;
        return Ok(profile);
    }

    if desired_path.exists() {
        return Err("这只龙虾的内部启动目录已存在，请先清理重复目录后重试。".to_string());
    }

    if fs::rename(&current_path, &desired_path).is_err() {
        copy_dir_recursive(&current_path, &desired_path)?;
        fs::remove_dir_all(&current_path).map_err(to_string_error)?;
    }

    normalize_managed_profile_runtime(&desired_path, &profile.id, Some(settings))?;

    Ok(ManagedProfile {
        path: desired_path.display().to_string(),
        ..profile
    })
}

fn resolve_launch_target_in_cli_context(
    context: &LauncherContext,
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
                "没有找到默认资料目录，请先确认 C:\\Users\\用户名\\.openclaw 是否存在。"
                    .to_string()
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
            profile_name: "默认龙虾".to_string(),
            profile_path: path.clone(),
            runtime_profile_path: path,
            cli_profile_name: None,
            use_state_dir_env,
            managed_profile: None,
        });
    }

    let profile = list_profiles_in_cli_context(context)?
        .into_iter()
        .find(|item| item.id == profile_id)
        .ok_or_else(|| "没有找到要启动的资料。".to_string())?;
    let profile = ensure_managed_profile_launch_path_in_cli_context(settings, profile)?;
    if !Path::new(&profile.path).is_dir() {
        return Err("选中的资料目录不存在，请重新导入。".into());
    }
    Ok(LaunchTarget {
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        profile_path: profile.path.clone(),
        runtime_profile_path: profile.path.clone(),
        cli_profile_name: Some(cli_profile_name_for(&profile.name, &profile.id)),
        use_state_dir_env: false,
        managed_profile: Some(profile),
    })
}

fn import_profile_in_cli_context(
    context: &LauncherContext,
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

    let mut settings = load_settings_in_cli_context(context)?;
    let root = profiles_root_in_cli_context(&settings)?;
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
    normalize_managed_profile_runtime(&temp_target, &profile.id, Some(&settings))?;
    fs::rename(&temp_target, &target).map_err(to_string_error)?;

    settings.recent_profile_id = Some(profile.id.clone());
    save_settings_in_cli_context(context, settings)?;
    Ok(profile)
}

fn launch_profile_in_cli(
    context: &LauncherContext,
    profile_id: Option<String>,
) -> Result<LaunchHandle, String> {
    let mut settings = load_settings_in_cli_context(context)?;
    let selected_profile = profile_id
        .or_else(|| settings.recent_profile_id.clone())
        .unwrap_or_else(|| LOCAL_PROFILE_ID.to_string());
    let executable = settings
        .openclaw_executable_path
        .clone()
        .ok_or_else(|| "未找到 OpenClaw 启动入口，请先确认设置。".to_string())?;
    let executable_path = PathBuf::from(&executable);
    if !executable_path.exists() {
        return Err("当前配置的启动入口不存在，请重新检测或手动指定。".into());
    }

    let launch_target = resolve_launch_target_in_cli_context(context, &settings, &selected_profile)?;
    let (gateway_config, gateway_reused) =
        ensure_target_gateway_running_for_cli(&executable_path, &launch_target)?;

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

    settings.recent_profile_id = Some(launch_target.profile_id.clone());
    settings.gateway_config = gateway_config.clone();
    settings.recent_launches.insert(
        0,
        LaunchRecord {
            profile_id: launch_target.profile_id.clone(),
            profile_name: launch_target.profile_name.clone(),
            launched_at: started_at.clone(),
        },
    );
    settings.recent_launches.truncate(10);
    save_settings_in_cli_context(context, settings)?;

    if let Some(profile) = launch_target.managed_profile.clone() {
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
        pid: Some(child.id()),
        started_at,
        profile_id: launch_target.profile_id,
        profile_name: launch_target.profile_name,
        executable_path: executable,
        args,
        connection_message: Some(if gateway_reused {
            "已复用当前连接服务。".to_string()
        } else {
            "已为当前龙虾启动独立连接服务。".to_string()
        }),
    })
}

fn ensure_target_gateway_running_for_cli(
    executable_path: &Path,
    launch_target: &LaunchTarget,
) -> Result<(GatewayConfig, bool), String> {
    let gateway_config = gateway_config_for_target(executable_path, launch_target)?;
    if health_check(&gateway_config).is_ok() {
        return Ok((gateway_config, true));
    }

    let command_path = PathBuf::from(
        gateway_config
            .command
            .clone()
            .ok_or_else(|| "缺少 OpenClaw 启动入口。".to_string())?,
    );
    if !command_path.exists() {
        return Err("用于启动连接服务的 OpenClaw 命令不存在。".into());
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
    let _ = process.spawn().map_err(to_string_error)?;

    health_check_with_retry(&gateway_config, 8, std::time::Duration::from_millis(600))?;
    Ok((gateway_config, false))
}

fn send_chat_message_in_cli(
    context: &LauncherContext,
    args: ChatSendArgs,
) -> Result<CliChatResult, String> {
    let mut settings = load_settings_in_cli_context(context)?;
    let profile_id = args
        .profile_id
        .or_else(|| settings.recent_profile_id.clone())
        .unwrap_or_else(|| LOCAL_PROFILE_ID.to_string());
    let conversation_id = args.conversation_id.unwrap_or_else(|| {
        format!(
            "{}--conv--{}",
            profile_session_key(&profile_id),
            Uuid::new_v4().simple()
        )
    });
    let request = ChatRequest {
        conversation_id: conversation_id.clone(),
        content: args.message,
        profile_id: Some(profile_id.clone()),
        model: None,
        params: None,
    };
    let conversation_path = cli_conversations_root(context).join(format!("{conversation_id}.json"));

    let mut conversation = if conversation_path.exists() {
        read_json::<Conversation>(&conversation_path)?
    } else {
        Conversation {
            id: conversation_id.clone(),
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
    write_json(&conversation_path, &conversation)?;

    let content = run_agent_cli_chat_in_cli_context(context, &settings, &request)?;
    conversation.messages.push(ChatMessage {
        id: Uuid::new_v4().to_string(),
        role: "assistant".into(),
        content: content.clone(),
        created_at: now_iso(),
    });
    conversation.updated_at = now_iso();
    sort_conversation_messages(&mut conversation);
    write_json(&conversation_path, &conversation)?;
    write_json(
        &cli_conversations_root(context)
            .join("_summaries")
            .join(format!("{conversation_id}.json")),
        &conversation_to_summary(&conversation),
    )?;

    settings.recent_profile_id = Some(profile_id.clone());
    save_settings_in_cli_context(context, settings)?;

    Ok(CliChatResult {
        profile_id,
        conversation_id,
        content,
    })
}

fn run_agent_cli_chat_in_cli_context(
    context: &LauncherContext,
    settings: &AppSettings,
    request: &ChatRequest,
) -> Result<String, String> {
    let executable = settings
        .openclaw_executable_path
        .clone()
        .ok_or_else(|| "未找到 OpenClaw 启动入口，请先确认设置。".to_string())?;
    let executable_path = PathBuf::from(&executable);
    if !executable_path.exists() {
        return Err("当前配置的 OpenClaw 启动入口不存在，请重新检测或手动指定。".into());
    }

    let profile_id = request
        .profile_id
        .clone()
        .or_else(|| settings.recent_profile_id.clone())
        .unwrap_or_else(|| LOCAL_PROFILE_ID.to_string());
    let launch_target = resolve_launch_target_in_cli_context(context, settings, &profile_id)?;
    let (gateway_config, _) = ensure_target_gateway_running_for_cli(&executable_path, &launch_target)?;

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
        return Err(format!(
            "{}\nGateway URL: {}",
            agent_cli_error_message(&stdout, &stderr),
            gateway_config.url
        ));
    }

    let response = parsed_response.ok_or_else(|| {
        format!(
            "无法解析龙虾回复.\n{}\nGateway URL: {}",
            raw_output_excerpt(&stdout, &stderr),
            gateway_config.url
        )
    })?;
    let content = extract_agent_cli_text(&response);
    if content.trim().is_empty() {
        Ok("龙虾暂时没有返回内容.".to_string())
    } else {
        Ok(content)
    }
}
