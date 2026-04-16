const puppeteer = require('puppeteer');

async function debugEditor() {
    console.log("Launching headless browser...");
    const browser = await puppeteer.launch({ headless: 'new' });
    const page = await browser.newPage();
    
    // Pipe browser console to our terminal
    page.on('console', msg => console.log(`[Browser Console] ${msg.type()}: ${msg.text()}`));
    page.on('pageerror', error => console.error(`[Browser Error] ${error.message}`));
    page.on('requestfailed', request => {
        console.error(`[Network Error] Failed to load ${request.url()} - ${request.failure().errorText}`);
    });

    try {
        console.log("Navigating to http://localhost:1420...");
        await page.goto('http://localhost:1420', { waitUntil: 'networkidle2' });
        await new Promise(r => setTimeout(r, 1000)); // Wait for WASM to boot

        console.log("Clicking 'Journal' tab...");
        await page.evaluate(() => {
            const btns = Array.from(document.querySelectorAll('button'));
            const journalBtn = btns.find(b => b.textContent.includes('Journal'));
            if (journalBtn) journalBtn.click();
        });
        await new Promise(r => setTimeout(r, 1000));

        console.log("Clicking 'New Note' to trigger Editor...");
        await page.evaluate(() => {
            const btns = Array.from(document.querySelectorAll('button'));
            const newNoteBtn = btns.find(b => b.textContent.includes('New Note'));
            if (newNoteBtn) newNoteBtn.click();
        });
        
        // Wait 2 seconds to capture any delayed crashes
        console.log("Waiting to capture crash logs...");
        await new Promise(r => setTimeout(r, 2000));
        
    } catch (e) {
        console.error("Test script failed:", e);
    } finally {
        await browser.close();
        console.log("Browser closed.");
    }
}

debugEditor();