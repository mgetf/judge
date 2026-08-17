//! Discord-first court tour. Screenshots the guild UX after each act.
//!
//!   cargo run --example screenshot_tour
//!
//! Writes PNGs to `./screenshots` and, if present, `/opt/cursor/artifacts/screenshots`.

use std::path::{Path, PathBuf};

use judge::testing::start_stack;
use playwright_rs::protocol::browser_context::Viewport;
use playwright_rs::protocol::{ScreenshotOptions, ScreenshotType};
use playwright_rs::{expect, Playwright};

fn out_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("screenshots")];
    let artifacts = PathBuf::from("/opt/cursor/artifacts/screenshots");
    if Path::new("/opt/cursor/artifacts").is_dir() || std::fs::create_dir_all(&artifacts).is_ok() {
        if !dirs.contains(&artifacts) {
            dirs.push(artifacts);
        }
    }
    dirs
}

async fn snap(
    page: &playwright_rs::protocol::page::Page,
    dirs: &[PathBuf],
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let opts = ScreenshotOptions::builder()
        .screenshot_type(ScreenshotType::Png)
        .full_page(true)
        .build();
    let bytes = page.screenshot(Some(opts)).await?;
    for dir in dirs {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{name}.png"));
        std::fs::write(&path, &bytes)?;
        eprintln!("wrote {}", path.display());
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dirs = out_dirs();
    let stack = start_stack().await?;

    if let Err(e) = playwright_rs::install_browsers(Some(&["chromium"])).await {
        eprintln!("install_browsers: {e} (continuing if already installed)");
    }

    let pw = Playwright::launch().await?;
    let browser = pw.chromium().launch().await?;
    let page = browser.new_page().await?;
    page.set_viewport_size(Viewport {
        width: 1100,
        height: 800,
    })
    .await?;

    page.goto(&stack.mock.base_url, None).await?;
    snap(&page, &dirs, "01-pick-member").await?;

    page.locator("[data-testid=continue-tommyy]")
        .click(None)
        .await?;
    expect(page.locator("[data-testid=guild]"))
        .to_be_visible()
        .await?;
    snap(&page, &dirs, "02-docket-channel").await?;

    page.locator("[data-testid=open-case]").click(None).await?;
    expect(page.locator("[data-testid=modal]"))
        .to_be_visible()
        .await?;
    page.locator("[data-testid=modal] [data-testid=case-id]")
        .fill("case-cheat-1", None)
        .await?;
    page.locator("[data-testid=modal] [data-testid=case-brief]")
        .fill("STV aimbot on mge_training", None)
        .await?;
    page.locator("[data-testid=modal] [data-testid=case-subject]")
        .fill("76561198000000000", None)
        .await?;
    snap(&page, &dirs, "03-open-case-modal").await?;

    page.locator("[data-testid=open-case-submit]")
        .click(None)
        .await?;
    expect(page.locator("[data-testid=channel-case-cheat-1]"))
        .to_be_visible()
        .await?;
    page.locator("[data-testid=channel-case-cheat-1]")
        .click(None)
        .await?;
    snap(&page, &dirs, "04-case-channel-intake").await?;

    page.locator("[data-testid=attach-submit]")
        .click(None)
        .await?;
    snap(&page, &dirs, "05-attachment-evidence").await?;

    page.locator("[data-testid=propose-outcome]")
        .click(None)
        .await?;
    page.locator("[data-testid=outcome-id]")
        .fill("cheat-ban", None)
        .await?;
    page.locator("[data-testid=outcome-body]")
        .fill("Permanent cheat ban", None)
        .await?;
    page.locator("[data-testid=outcome-submit]")
        .click(None)
        .await?;
    page.locator("[data-testid=propose-outcome]")
        .click(None)
        .await?;
    page.locator("[data-testid=outcome-id]")
        .fill("no-action", None)
        .await?;
    page.locator("[data-testid=outcome-body]")
        .fill("Not convinced", None)
        .await?;
    page.locator("[data-testid=outcome-submit]")
        .click(None)
        .await?;
    snap(&page, &dirs, "06-outcomes").await?;

    page.locator("[data-testid=deliberate-submit]")
        .click(None)
        .await?;
    expect(page.locator("[data-testid=case-phase]"))
        .to_contain_text("deliberation")
        .await?;
    snap(&page, &dirs, "07-deliberation").await?;

    page.locator("[data-testid=cast-vote]").click(None).await?;
    page.locator("[data-testid=vote-outcome]")
        .fill("cheat-ban", None)
        .await?;
    page.locator("[data-testid=vote-reason]")
        .fill("Demo is unambiguous.", None)
        .await?;
    page.locator("[data-testid=vote-submit]")
        .click(None)
        .await?;
    snap(&page, &dirs, "08-ballot").await?;

    page.locator("[data-testid=close-submit]")
        .click(None)
        .await?;
    expect(page.locator("[data-testid=verdict]"))
        .to_be_visible()
        .await?;
    snap(&page, &dirs, "09-verdict-in-channel").await?;

    page.goto(&format!("{}/cases/case-cheat-1", stack.judge_url), None)
        .await?;
    snap(&page, &dirs, "10-html-live-view").await?;

    let _ = browser.close().await;
    stack.abort();
    eprintln!("done. shots in {:?}", dirs);
    Ok(())
}
