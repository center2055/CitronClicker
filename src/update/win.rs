use std::io::Read;
use std::path::Path;

const REPO: &str = "center2055/CitronClicker";
const UA: &str = "Citron-v2-updater";

#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}
#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// delete the renamed old binary left by a previous update. always safe to call.
pub fn startup_cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
    }
}

/// background-check github for a newer release and stage it for the next launch. silent —
/// every failure path (private repo, no release, no network, bad download) is a no-op.
pub fn spawn_check() {
    std::thread::spawn(|| {
        let _ = check_and_stage();
    });
}

fn check_and_stage() -> Option<()> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let release: Release = ureq::get(&url)
        .set("User-Agent", UA)
        .set("Accept", "application/vnd.github+json")
        .call()
        .ok()?
        .into_json()
        .ok()?;

    let latest = release.tag_name.trim_start_matches(['v', 'V']);
    if !is_newer(latest, env!("CARGO_PKG_VERSION")) {
        return None;
    }

    // first .exe asset on the release
    let asset = release
        .assets
        .iter()
        .find(|a| a.name.to_ascii_lowercase().ends_with(".exe"))?;

    let exe = std::env::current_exe().ok()?;
    let new_path = exe.with_extension("new");
    download(&asset.browser_download_url, &new_path).ok()?;

    // sanity: must be a real windows exe ("MZ" header) and a sane size, else leave ours alone
    if !looks_like_exe(&new_path) {
        let _ = std::fs::remove_file(&new_path);
        return None;
    }

    // a running exe can't be overwritten, but it can be renamed: move ours aside, drop the new
    // one into place. the live process keeps running from the renamed file; next launch is new.
    let old_path = exe.with_extension("old");
    let _ = std::fs::remove_file(&old_path);
    if std::fs::rename(&exe, &old_path).is_err() {
        let _ = std::fs::remove_file(&new_path);
        return None;
    }
    if std::fs::rename(&new_path, &exe).is_err() {
        let _ = std::fs::rename(&old_path, &exe); // recover — put ours back
        let _ = std::fs::remove_file(&new_path);
        return None;
    }
    Some(())
}

fn download(url: &str, to: &Path) -> Result<(), ()> {
    let resp = ureq::get(url).set("User-Agent", UA).call().map_err(|_| ())?;
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(to).map_err(|_| ())?;
    std::io::copy(&mut reader, &mut file).map_err(|_| ())?;
    Ok(())
}

fn looks_like_exe(p: &Path) -> bool {
    let big_enough = std::fs::metadata(p).map(|m| m.len() > 4096).unwrap_or(false);
    let mut head = [0u8; 2];
    let mz = std::fs::File::open(p)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && &head == b"MZ";
    big_enough && mz
}

/// numeric dotted compare (0.2.10 > 0.2.9); missing parts count as 0.
fn is_newer(latest: &str, current: &str) -> bool {
    fn parts(s: &str) -> Vec<u64> {
        s.split('.')
            .map(|p| p.trim_matches(|c: char| !c.is_ascii_digit()).parse().unwrap_or(0))
            .collect()
    }
    let (l, c) = (parts(latest), parts(current));
    for i in 0..l.len().max(c.len()) {
        let (lv, cv) = (l.get(i).copied().unwrap_or(0), c.get(i).copied().unwrap_or(0));
        if lv != cv {
            return lv > cv;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn version_compare() {
        assert!(is_newer("0.3.0", "0.2.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.2.10", "0.2.9")); // numeric, not lexical
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.5", "0.2.0"));
        assert!(is_newer("v0.4".trim_start_matches('v'), "0.3.9"));
    }
}
