const axios = require("axios");
const https = require('https');
const puppeteer = require('puppeteer');

(async () => {
  const browser = await puppeteer.launch({
    ignoreHTTPSErrors: true,
    headless: true
  });
  const page = await browser.newPage();
  const baseUrl = 'https://localhost:8443';
  console.log("Gathering performance metrics for", baseUrl);
  await page.goto(baseUrl);
  let pageMetrics = await page.metrics();
  const performanceTiming = JSON.parse(
    await page.evaluate(() => JSON.stringify(window.performance.timing))
  );
  const performanceEntries = JSON.parse(
    await page.evaluate(() => JSON.stringify(window.performance.getEntries()))
  );
  const resource_entries = performanceEntries.filter(e => e.entryType === 'resource' || e.entryType === 'navigation');
  const metrics = {
    numberOfResources: pageMetrics.Documents,
    transferSize: resource_entries.reduce((acc, curr) => acc + curr.transferSize, 0),
    encodedBodySize: resource_entries.reduce((acc, curr) => acc + curr.encodedBodySize, 0),
    decodedBodySize: resource_entries.reduce((acc, curr) => acc + curr.decodedBodySize, 0),
    backendTime: performanceTiming.responseStart - performanceTiming.navigationStart,
    timeToFirstByte: performanceTiming.loadEventStart - performanceTiming.navigationStart,
    timeToStartRender: performanceTiming.domLoading - performanceTiming.navigationStart,
    timeToInteractive: performanceTiming.domInteractive - performanceTiming.navigationStart,
    resourceDownloadTime: performanceTiming.responseEnd - performanceTiming.requestStart,
    dnsLookupTime: performanceTiming.domainLookupEnd -performanceTiming.domainLookupStart
  }
  let currentTime = Math.floor(Date.now() / 1000);
  await browser.close();
  console.log("Using", baseUrl, "as base URL for publishing metrics.");
  console.log("Sending metrics:", metrics);
  const instance = axios.create({
    httpsAgent: new https.Agent({
      rejectUnauthorized: false
    })
  });
  for (const metric in metrics) {
    if (metrics.hasOwnProperty(metric)) {
      let url = baseUrl + '/' + metric;
      let data = {
        timeStamp: currentTime,
        value: metrics[metric]
      };
      let response = await instance.post(url, data);
      console.log("Recieved acknowledgement:", response.data);
    }
  }
})();
