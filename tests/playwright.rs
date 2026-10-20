mod common;

use playwright_rs::{expect, Playwright};

#[tokio::test(flavor = "multi_thread")]
async fn chief_records_a_cheat_verdict_in_discord() {
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

    page.goto(&format!("{}/app?as_user=100", stack.mock.base_url), None)
        .await
        .expect("goto guild");
    expect(page.locator("[data-testid=guild]"))
        .to_be_visible()
        .await
        .expect("guild");
    expect(page.locator("[data-testid=channel-docket]"))
        .to_be_visible()
        .await
        .expect("docket channel");

    page.locator("[data-testid=open-case]")
        .click(None)
        .await
        .expect("open case button");
    expect(page.locator("[data-testid=modal]"))
        .to_be_visible()
        .await
        .expect("modal");
    page.locator("[data-testid=modal] [data-testid=case-title-field]")
        .fill("case-cheat-1", None)
        .await
        .expect("case title");
    page.locator("[data-testid=modal] [data-testid=case-brief]")
        .fill("STV aimbot on mge_training", None)
        .await
        .expect("brief");
    page.locator("[data-testid=modal] [data-testid=case-subject]")
        .fill("76561198000000000", None)
        .await
        .expect("subject");
    page.locator("[data-testid=open-case-submit]")
        .click(None)
        .await
        .expect("submit case");

    expect(page.locator("[data-testid=channel-case-cheat-1]"))
        .to_be_visible()
        .await
        .expect("case channel");
    page.locator("[data-testid=channel-case-cheat-1]")
        .click(None)
        .await
        .expect("open case channel");
    expect(page.locator("[data-testid=case-title]"))
        .to_have_text("STV aimbot on mge_training")
        .await
        .expect("live case view");
    expect(page.locator("[data-testid=case-phase]"))
        .to_contain_text("intake")
        .await
        .expect("intake");

    page.locator("[data-testid=attach-filename]")
        .fill("demo.dem", None)
        .await
        .unwrap();
    page.locator("[data-testid=attach-url]")
        .fill("https://mge.tf/demos/abc", None)
        .await
        .unwrap();
    page.locator("[data-testid=attach-submit]")
        .click(None)
        .await
        .unwrap();
    expect(page.locator("[data-testid=messages]"))
        .to_contain_text("demo.dem")
        .await
        .expect("attachment posted in chat");

    page.locator("[data-testid=propose-outcome]")
        .click(None)
        .await
        .unwrap();
    page.locator("[data-testid=modal] [data-testid=outcome-body]")
        .fill("Permanent cheat ban", None)
        .await
        .unwrap();
    page.locator("[data-testid=outcome-submit]")
        .click(None)
        .await
        .unwrap();

    page.locator("[data-testid=propose-outcome]")
        .click(None)
        .await
        .unwrap();
    page.locator("[data-testid=modal] [data-testid=outcome-body]")
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
        .to_contain_text("deliberation")
        .await
        .expect("deliberation");

    page.locator("[data-testid=cast-vote]")
        .click(None)
        .await
        .unwrap();
    page.locator("[data-testid=modal] [data-testid=vote-outcome]")
        .fill("permanent-cheat-ban", None)
        .await
        .unwrap();
    page.locator("[data-testid=modal] [data-testid=vote-reason]")
        .fill("Demo is unambiguous.", None)
        .await
        .unwrap();
    page.locator("[data-testid=vote-submit]")
        .click(None)
        .await
        .unwrap();

    page.locator("[data-testid=close-request]")
        .click(None)
        .await
        .unwrap();
    expect(page.locator("[data-testid=close-submit]"))
        .to_be_visible()
        .await
        .expect("confirm close");
    expect(page.locator("[data-testid=close-cancel]"))
        .to_be_visible()
        .await
        .expect("cancel close");
    page.locator("[data-testid=close-submit]")
        .click(None)
        .await
        .unwrap();
    expect(page.locator("[data-testid=verdict]"))
        .to_be_visible()
        .await
        .expect("verdict snapshot");
    expect(page.locator("[data-testid=winner]"))
        .to_contain_text("permanent-cheat-ban")
        .await
        .expect("winner");
    page.goto(&format!("{}/app?as_user=100", stack.mock.base_url), None)
        .await
        .expect("reload guild for channel rename");
    expect(page.locator("[data-testid=channel-closed-case-cheat-1]"))
        .to_be_visible()
        .await
        .expect("closed ticket rename");

    // The web mirror is private to the bench: log in with Discord first.
    page.goto(&format!("{}/login", stack.judge_url), None)
        .await
        .expect("goto login");
    page.locator("[data-testid=continue-tommyy]")
        .click(None)
        .await
        .expect("continue as tommyy");
    expect(page.locator("[data-testid=bench]"))
        .to_be_visible()
        .await
        .expect("logged-in docket");
    page.goto(&format!("{}/cases/case-cheat-1", stack.judge_url), None)
        .await
        .expect("goto closed case");
    expect(page.locator("[data-testid=chat-archive]"))
        .to_contain_text("demo.dem")
        .await
        .expect("website shows archived chat");

    let _ = browser.close().await;
    stack.abort();
}
