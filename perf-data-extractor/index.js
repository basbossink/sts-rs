const puppeteer = require('puppeteer');

(async () => {
  const browser = await puppeteer.launch({
    ignoreHTTPSErrors: true,
    headless: true
  });
  const page = await browser.newPage();
  await page.goto('https://localhost:8443');
  let page_metrics = await page.metrics();
  console.log("page metrics", page_metrics);
  const performanceTiming = JSON.parse(
    await page.evaluate(() => JSON.stringify(window.performance.timing))
  );
  const performanceEntries = JSON.parse(
    await page.evaluate(() => JSON.stringify(window.performance.getEntries()))
  );
  const resource_entries = performanceEntries.filter(e => e.entryType === 'resource' || e.entryType === 'navigation');
  const size_metrics = {
    no_resources: page_metrics.Documents,
    transfer_size: resource_entries.reduce((acc, curr) => acc + curr.transferSize, 0),
    encoded_body_size: resource_entries.reduce((acc, curr) => acc + curr.encodedBodySize, 0),
    decoded_body_size: resource_entries.reduce((acc, curr) => acc + curr.decodedBodySize, 0),
  }
  const perf_timings = {
    backend_time: performanceTiming.responseStart - performanceTiming.navigationStart,
    time_to_first_byte: performanceTiming.loadEventStart - performanceTiming.navigationStart,
    time_to_start_render: performanceTiming.domLoading - performanceTiming.navigationStart,
    time_to_interactive: performanceTiming.domInteractive - performanceTiming.navigationStart,
    resource_download_time: performanceTiming.responseEnd - performanceTiming.requestStart,
    dns_lookup_time: performanceTiming.domainLookupEnd -performanceTiming.domainLookupStart
  };
  console.log("raw timing ", performanceTiming);
  console.log("calulated metrics ", perf_timings);
  console.log("size_metrics", size_metrics);
  await browser.close();
})();
