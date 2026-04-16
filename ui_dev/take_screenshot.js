const puppeteer = require('puppeteer');
const fs = require('fs');

async function takeScreenshot() {
    const url = process.argv[2] || 'http://localhost:1420';
    const outputPath = 'ui_state.png';
    
    console.log(`Navigating to ${url}...`);
    
    try {
        const browser = await puppeteer.launch({
            headless: 'new',
            // Samsung Galaxy S21 5G viewport
            defaultViewport: {
                width: 360,
                height: 800,
                isMobile: true,
                hasTouch: true,
                deviceScaleFactor: 3
            }
        });
        
        const page = await browser.newPage();
        
        // Go to the URL and wait for network to settle
        await page.goto(url, { waitUntil: 'networkidle0', timeout: 5000 }).catch(e => {
             console.log("Navigation timeout or error, continuing to screenshot anyway:", e.message);
        });
        
        // Give Dioxus an extra second to render any client-side WASM DOM updates
        await new Promise(resolve => setTimeout(resolve, 1000));
        
        await page.screenshot({ path: outputPath });
        console.log(`Screenshot saved to ${outputPath}`);
        
        await browser.close();
    } catch (error) {
        console.error("Failed to take screenshot:", error);
        process.exit(1);
    }
}

takeScreenshot();