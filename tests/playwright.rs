mod common;

use playwright_rs::protocol::{DropOptions, FilePayload};
use playwright_rs::{expect, Playwright};

#[tokio::test(flavor = "multi_thread")]
async fn chief_records_a_cheat_verdict() {
    let stack = common::start_stack().await.expect("stack");

    if let Err(e) = playwright_rs::install_browsers(Some(&["chromium"])).await {
        eprintln!("install_browsers: {e} (continuing if already installed)");
    }

    let pw = Playwright::launch().await.expect("playwright driver");
    let browser = pw
        .chromium()
        .launch()
        .await
        .expect("chromium — run: npx playwright@1.61.1 install chromium");
    let page = browser.new_page().await.expect("page");

    page.goto(&stack.judge_url, None)
        .await
        .expect("goto docket");
    page.locator("[data-testid=login]")
        .click(None)
        .await
        .expect("click login");
    page.locator("[data-testid=continue-tommyy]")
        .click(None)
        .await
        .expect("authorize as tommyy");

    expect(page.locator("[data-testid=whoami]"))
        .to_have_text("tommyy")
        .await
        .expect("logged in");
    expect(page.locator("[data-testid=seat]"))
        .to_contain_text("chief")
        .await
        .expect("chief seat from discord role");

    page.locator("[data-testid=case-id]")
        .fill("case-cheat-1", None)
        .await
        .expect("case id");
    page.locator("[data-testid=case-brief]")
        .fill("STV aimbot on mge_training", None)
        .await
        .expect("brief");
    page.locator("[data-testid=case-subject]")
        .fill("76561198000000000", None)
        .await
        .expect("subject");
    page.locator("[data-testid=open-case-submit]")
        .click(None)
        .await
        .expect("open case");

    expect(page.locator("[data-testid=case-title]"))
        .to_have_text("case-cheat-1")
        .await
        .expect("on case page");
    expect(page.locator("[data-testid=case-phase]"))
        .to_have_text("intake")
        .await
        .expect("intake");

    page.locator("[data-testid=evidence-id]")
        .fill("demo-stv", None)
        .await
        .unwrap();
    page.locator("[data-testid=evidence-label]")
        .fill("STV demo", None)
        .await
        .unwrap();
    page.locator("[data-testid=evidence-body]")
        .fill("https://mge.tf/demos/abc snap at 3:12", None)
        .await
        .unwrap();
    let snap = std::fs::read("tests/fixtures/stv-snap.png").expect("exhibit fixture");
    page.locator("[data-testid=evidence-drop]")
        .drop(
            DropOptions::builder()
                .file(FilePayload::new(
                    "stv-snap.png",
                    "image/png",
                    snap,
                ))
                .build(),
        )
        .await
        .expect("drop exhibit");
    expect(page.locator("[data-testid=evidence-picked]"))
        .to_contain_text("stv-snap.png")
        .await
        .expect("jquery took the drop");
    page.locator("[data-testid=evidence-submit]")
        .click(None)
        .await
        .unwrap();

    page.locator("[data-testid=outcome-id]")
        .fill("cheat-ban", None)
        .await
        .unwrap();
    page.locator("[data-testid=outcome-body]")
        .fill("Permanent cheat ban", None)
        .await
        .unwrap();
    page.locator("[data-testid=outcome-submit]")
        .click(None)
        .await
        .unwrap();

    page.locator("[data-testid=outcome-id]")
        .fill("no-action", None)
        .await
        .unwrap();
    page.locator("[data-testid=outcome-body]")
        .fill("Not convinced", None)
        .await
        .unwrap();
    page.locator("[data-testid=outcome-submit]")
        .click(None)
        .await
        .unwrap();

    page.locator("[data-testid=deliberate-submit]")
        .click(None)
        .await
        .unwrap();
    expect(page.locator("[data-testid=case-phase]"))
        .to_have_text("deliberation")
        .await
        .expect("deliberation");

    page.locator("[data-testid=vote-outcome]")
        .select_option("cheat-ban", None)
        .await
        .unwrap();
    page.locator("[data-testid=vote-reason]")
        .fill("Demo is unambiguous.", None)
        .await
        .unwrap();
    page.locator("[data-testid=vote-submit]")
        .click(None)
        .await
        .unwrap();

    page.locator("[data-testid=close-submit]")
        .click(None)
        .await
        .unwrap();

    expect(page.locator("[data-testid=verdict]"))
        .to_be_visible()
        .await
        .expect("verdict snapshot");
    expect(page.locator("[data-testid=winner]"))
        .to_contain_text("cheat-ban")
        .await
        .expect("winner");
    expect(page.locator("[data-testid=evidence-demo-stv]"))
        .to_be_visible()
        .await
        .expect("evidence remains linkable");
    expect(page.locator("[data-testid=exhibit-demo-stv]"))
        .to_be_visible()
        .await
        .expect("exhibit filed");
    expect(page.locator("[data-testid=exhibit-img-demo-stv]"))
        .to_be_visible()
        .await
        .expect("image exhibit");

    let _ = browser.close().await;
    stack.abort();
}
