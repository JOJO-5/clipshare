use serde::Serialize;
use tauri::{command, AppHandle};
use tauri_plugin_shell::ShellExt;

const RELEASES: &str = "https://github.com/JOJO-5/clipshare/releases";

#[derive(Clone, Copy, Debug, PartialEq)]
enum Package {
    Win7Nsis,
    ModernNsis,
    ModernMsi,
}

impl Package {
    fn installed() -> Option<Self> {
        use tauri::utils::{config::BundleType, platform::bundle_type};

        if !cfg!(all(windows, target_arch = "x86_64")) {
            return None;
        }
        match (cfg!(feature = "win7-compat"), bundle_type()) {
            (true, Some(BundleType::Nsis)) => Some(Self::Win7Nsis),
            (false, Some(BundleType::Nsis)) => Some(Self::ModernNsis),
            (false, Some(BundleType::Msi)) => Some(Self::ModernMsi),
            _ => None,
        }
    }

    #[cfg(windows)]
    fn target(self) -> &'static str {
        match self {
            Self::Win7Nsis => "windows-win7-x86_64",
            Self::ModernNsis => "windows-x86_64-nsis",
            Self::ModernMsi => "windows-x86_64-msi",
        }
    }

    #[cfg(any(windows, test))]
    fn accepts_url(self, version: &str, url: &str) -> bool {
        if version.contains(['-', '+']) {
            return false;
        }
        let filename = match self {
            Self::Win7Nsis => format!("ClipShare.Win7_{version}_x64-setup.exe"),
            Self::ModernNsis => format!("ClipShare_{version}_x64-setup.exe"),
            Self::ModernMsi => format!("ClipShare_{version}_x64_en-US.msi"),
        };
        url == format!("{RELEASES}/download/v{version}/{filename}")
    }
}

#[derive(Serialize)]
pub struct UpdateInfo {
    version: String,
    supported: bool,
}

#[command]
pub fn get_update_info(app: AppHandle) -> UpdateInfo {
    UpdateInfo {
        version: app.package_info().version.to_string(),
        supported: Package::installed().is_some(),
    }
}

#[command]
#[allow(deprecated)]
pub fn open_release_downloads(app: AppHandle) -> Result<(), String> {
    app.shell()
        .open(RELEASES, None)
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::time::Duration;
    use tauri::{ipc::Channel, State};
    use tauri_plugin_updater::{Error, Update, UpdaterExt};

    #[derive(Default)]
    pub struct UpdateInstallLock(tokio::sync::Mutex<()>);

    #[derive(Serialize)]
    #[serde(tag = "state", rename_all = "snake_case")]
    pub enum UpdateCheck {
        Current,
        NoManifest,
        Available { version: String, notes: String },
    }

    #[derive(Clone, Serialize)]
    #[serde(tag = "state", rename_all = "snake_case")]
    pub enum Progress {
        Downloading { downloaded: u64, total: Option<u64> },
        Verifying,
        Installing,
    }

    async fn find_update(app: &AppHandle) -> Result<Option<Update>, Error> {
        let Some(package) = Package::installed() else {
            return Ok(None);
        };
        let update = app
            .updater_builder()
            .target(package.target())
            .timeout(Duration::from_secs(15))
            .build()?
            .check()
            .await?;
        if let Some(ref candidate) = update {
            if !package.accepts_url(&candidate.version, candidate.download_url.as_str()) {
                return Err(Error::Network(
                    "更新清单中的安装包与当前版本类型不匹配。".into(),
                ));
            }
        }
        Ok(update)
    }

    #[command]
    pub async fn check_for_update(app: AppHandle) -> Result<UpdateCheck, String> {
        match find_update(&app).await {
            Ok(Some(update)) => Ok(UpdateCheck::Available {
                version: update.version,
                notes: update.body.unwrap_or_default(),
            }),
            Ok(None) => Ok(UpdateCheck::Current),
            Err(Error::ReleaseNotFound) => Ok(UpdateCheck::NoManifest),
            Err(error) => Err(error.to_string()),
        }
    }

    #[command]
    pub async fn install_app_update(
        app: AppHandle,
        lock: State<'_, UpdateInstallLock>,
        version: String,
        progress: Channel<Progress>,
    ) -> Result<(), String> {
        let _installing = lock
            .0
            .try_lock()
            .map_err(|_| "更新正在进行。".to_string())?;
        let mut update = find_update(&app)
            .await
            .map_err(|error| error.to_string())?
            .filter(|update| update.version == version)
            .ok_or_else(|| "可用版本已变化，请重新检查更新。".to_string())?;
        update.timeout = Some(Duration::from_secs(900));

        let mut downloaded = 0u64;
        let mut reported = 0u64;
        let bytes = update
            .download(
                |size, total| {
                    downloaded += size as u64;
                    if downloaded - reported >= 512 * 1024 || Some(downloaded) == total {
                        let _ = progress.send(Progress::Downloading { downloaded, total });
                        reported = downloaded;
                    }
                },
                || {
                    let _ = progress.send(Progress::Verifying);
                },
            )
            .await
            .map_err(|error| match error {
                Error::Minisign(_) | Error::Base64(_) | Error::SignatureUtf8(_) => {
                    "安装包签名校验失败，请在 GitHub 下载官方安装包。".to_string()
                }
                other => other.to_string(),
            })?;

        let _ = progress.send(Progress::Installing);
        update.install(bytes).map_err(|error| error.to_string())
    }
}

#[cfg(windows)]
pub use windows::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_win7_install_only_accepts_its_versioned_github_installer() {
        let win7 = "https://github.com/JOJO-5/clipshare/releases/download/v0.1.5/ClipShare.Win7_0.1.5_x64-setup.exe";
        assert!(Package::Win7Nsis.accepts_url("0.1.5", win7));
        assert!(!Package::ModernNsis.accepts_url("0.1.5", win7));
        assert!(!Package::Win7Nsis.accepts_url("0.1.6", win7));
        assert!(!Package::Win7Nsis.accepts_url("0.1.5-test.1", win7));
        assert!(!Package::Win7Nsis.accepts_url("0.1.5", &win7.replace("github.com", "example.org")));
    }

    #[test]
    fn modern_msi_and_nsis_cannot_be_interchanged() {
        let msi = "https://github.com/JOJO-5/clipshare/releases/download/v0.1.5/ClipShare_0.1.5_x64_en-US.msi";
        assert!(Package::ModernMsi.accepts_url("0.1.5", msi));
        assert!(!Package::ModernNsis.accepts_url("0.1.5", msi));
    }
}
