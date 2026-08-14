//! Same cheat-record flow as `tests/playwright.rs`, screenshot after every page.
//!
//!   cargo run --example screenshot_tour
//!
//! Writes PNGs to `./screenshots` and, if present, `/opt/cursor/artifacts/screenshots`.

use std::path::{Path, PathBuf};

use judge::testing::start_stack;
use playwright_rs::protocol::browser_context::Viewport;
use playwright_rs::protocol::{ScreenshotOptions, ScreenshotType};
use playwright_rs::protocol::{DropOptions, FilePayload};
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

    page.goto(&stack.judge_url, None).await?;
    snap(&page, &dirs, "01-docket-logged-out").await?;

    page.locator("[data-testid=login]").click(None).await?;
    expect(page.locator("[data-testid=mock-discord]"))
        .to_be_visible()
        .await?;
    snap(&page, &dirs, "02-mock-discord-authorize").await?;

    page.locator("[data-testid=continue-tommyy]")
        .click(None)
        .await?;
    expect(page.locator("[data-testid=whoami]"))
        .to_have_text("tommyy")
        .await?;
    snap(&page, &dirs, "03-docket-logged-in").await?;

    page.locator("[data-testid=case-id]")
        .fill("case-cheat-1", None)
        .await?;
    page.locator("[data-testid=case-brief]")
        .fill("STV aimbot on mge_training", None)
        .await?;
    page.locator("[data-testid=case-subject]")
        .fill("76561198000000000", None)
        .await?;
    snap(&page, &dirs, "04-open-case-form-filled").await?;

    page.locator("[data-testid=open-case-submit]")
        .click(None)
        .await?;
    expect(page.locator("[data-testid=case-title]"))
        .to_have_text("case-cheat-1")
        .await?;
    snap(&page, &dirs, "05-case-intake").await?;

    page.locator("[data-testid=evidence-id]")
        .fill("demo-stv", None)
        .await?;
    page.locator("[data-testid=evidence-label]")
        .fill("STV demo", None)
        .await?;
    page.locator("[data-testid=evidence-body]")
        .fill("https://mge.tf/demos/abc snap at 3:12", None)
        .await?;
    let exhibit = std::fs::read("tests/fixtures/stv-snap.png")?;
    page.locator("[data-testid=evidence-drop]")
        .drop(
            DropOptions::builder()
                .file(FilePayload::new(
                    "stv-snap.png",
                    "image/png",
                    exhibit,
                ))
                .build(),
        )
        .await?;
    expect(page.locator("[data-testid=evidence-picked]"))
        .to_contain_text("stv-snap.png")
        .await?;
    snap(&page, &dirs, "06-evidence-drop").await?;
    page.locator("[data-testid=evidence-submit]")
        .click(None)
        .await?;
    snap(&page, &dirs, "07-evidence-filed").await?;

    page.locator("[data-testid=outcome-id]")
        .fill("cheat-ban", None)
        .await?;
    page.locator("[data-testid=outcome-body]")
        .fill("Permanent cheat ban", None)
        .await?;
    page.locator("[data-testid=outcome-submit]")
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
    snap(&page, &dirs, "08-outcomes-proposed").await?;

    page.locator("[data-testid=deliberate-submit]")
        .click(None)
        .await?;
    expect(page.locator("[data-testid=case-phase]"))
        .to_have_text("deliberation")
        .await?;
    snap(&page, &dirs, "09-deliberation").await?;

    page.locator("[data-testid=vote-outcome]")
        .select_option("cheat-ban", None)
        .await?;
    page.locator("[data-testid=vote-reason]")
        .fill("Demo is unambiguous.", None)
        .await?;
    snap(&page, &dirs, "10-ballot-filled").await?;

    page.locator("[data-testid=vote-submit]")
        .click(None)
        .await?;
    snap(&page, &dirs, "11-ballot-cast").await?;

    page.locator("[data-testid=close-submit]")
        .click(None)
        .await?;
    expect(page.locator("[data-testid=verdict]"))
        .to_be_visible()
        .await?;
    snap(&page, &dirs, "12-verdict").await?;

    page.goto(&format!("{}/log", stack.judge_url), None).await?;
    snap(&page, &dirs, "13-event-log").await?;

    page.goto(&format!("{}/people/100", stack.judge_url), None)
        .await?;
    snap(&page, &dirs, "14-person-tommyy").await?;

    page.goto(&stack.judge_url, None).await?;
    snap(&page, &dirs, "15-docket-with-closed-case").await?;

    let _ = browser.close().await;
    stack.abort();
    eprintln!("done. {} shots in {:?}", 15, dirs);
    Ok(())
}
