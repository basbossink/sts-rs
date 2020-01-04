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
  const performanceEntries = JSON.parse(
    await page.evaluate(() => JSON.stringify(window.performance.getEntries()))
  );
  let currentTime = Math.floor(Date.now() / 1000);
  await browser.close();

  const resourceEntries = performanceEntries.filter(e => e.entryType === 'resource');
  const navigationEntry = performanceEntries.filter(e => e.entryType === 'navigation')[0];
  const earliestRequestStart = Math.min.apply(null, resourceEntries.map(e => e.requestStart));
  const latestResponseEnd = Math.max.apply(null, resourceEntries.map(e => e.responseEnd));
  const earliestResponseStart = Math.max.apply(null, resourceEntries.map(e => e.responseStart));

  const metrics = {
    numberOfResources: pageMetrics.Documents,
    transferSizeInBytes: resourceEntries.reduce((acc, curr) => acc + curr.transferSize, 0),
    encodedBodySizeInBytes: resourceEntries.reduce((acc, curr) => acc + curr.encodedBodySize, 0),
    decodedBodySizeInBytes: resourceEntries.reduce((acc, curr) => acc + curr.decodedBodySize, 0),
    timeToFirstByteInMilliSeconds: navigationEntry.responseStart - navigationEntry.startTime,
    timeToStartRenderInMilliSeconds: navigationEntry.domContentLoadedEventStart - navigationEntry.startTime,
    timeToDomCompleteInMilliSeconds: navigationEntry.domComplete - navigationEntry.startTime,
    resourceDownloadTimeInMilliSeconds: latestResponseEnd - earliestRequestStart,
    totalTaskTimeInSeconds: pageMetrics.TaskDuration,
    dnsLookupTimeInMilliSeconds: navigationEntry.domainLookupEnd - navigationEntry.domainLookupStart,
    connectionSetupTimeInMilliSeconds: navigationEntry.connectEnd - navigationEntry.connectStart,
    requestSendPlusResponseLatencyInMilliSeconds: navigationEntry.responseStart - navigationEntry.requestStart,
    tcpInitiationOverheadInMilliSeconds: navigationEntry.requestStart - navigationEntry.startTime,
    backendResponseTimeInMilliSeconds: navigationEntry.responseEnd - navigationEntry.responseStart
  };

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
