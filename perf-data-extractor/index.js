const axios = require('axios');
const consola = require('consola');
const https = require('https');
const puppeteer = require('puppeteer');

/**
 * An object combining the raw metrics available from puppeteer and the browser.
 * @typedef {Object} RawMetrics
 * @property {Object} pageMetrics - The metrics provided by the puppeteer API.
 * @property {Object[]} performanceEntries - The performance entries available from
 * the browser
 */

/**
 * This function gather the raw performance metrics from
 * a single navigation to a single url.
 *
 * @param {browser} browser - A puppeteer browser instance.
 * @param {string} url - The URL to navigate to gather the metrics.
 * @return {RawMetrics} a complex object containing the puppeteer page metrics
 * and the performance entries from the window.
 */
async function gatherRawMetrics(browser, url) {
  const page = await browser.newPage();
  consola.info('Gathering performance metrics for', url);
  await page.goto(url);
  const pageMetrics = await page.metrics();
  const performanceEntries = JSON.parse(
      await page.evaluate(() => JSON.stringify(window.performance.getEntries())),
  );
  await page.close();
  return {pageMetrics, performanceEntries};
}

/**
 * This function maps the result of {@link gatherMetrics} and converts it to a complex object,
 * containing the performance characteristics of the visited page.
 * @param {RawMetrics} rawMetrics - The raw metrics as retrieved by {@link gatherMetrics}
 * @return {object} The calculated metrics for the given {@link rawMetrics}
 */
function calculateMeasurements(rawMetrics) {
  const {pageMetrics, performanceEntries} = rawMetrics;
  const resourceEntries = performanceEntries.filter((e) => e.entryType === 'resource');
  const navigationEntry = performanceEntries.filter((e) => e.entryType === 'navigation')[0];
  const earliestRequestStart = Math.min.apply(null, resourceEntries.map((e) => e.requestStart));
  const latestResponseEnd = Math.max.apply(null, resourceEntries.map((e) => e.responseEnd));

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
    backendResponseTimeInMilliSeconds: navigationEntry.responseEnd - navigationEntry.responseStart,
  };
  return metrics;
}

/**
 * Posts the metrics given to end-points using the given baseUrl.
 * @param {Object} client - The http(s) client to use for posting the data.
 * @param {string} baseUrl - The base URL to which '/<metric name>' will be appended
 * for performing the post(s) of the metrics.
 * @param {*} metrics - The metrics to publish, each property of the object will
 * be posted as a separate metric.
 * @param {number} currentTime - The timestamp for which these metrics should be
 * published.
 */
async function postMetrics(client, baseUrl, metrics, currentTime) {
  consola.info('Using', baseUrl, 'as base URL for publishing metrics.');
  consola.info('Sending metrics:', metrics);
  for (const metric in metrics) {
    if (metrics.hasOwnProperty(metric)) {
      const url = baseUrl + '/' + metric;
      const data = {
        timeStamp: currentTime,
        value: metrics[metric],
      };
      const response = await client.post(url, data);
      consola.debug('Recieved acknowledgement:', response.data);
    }
  }

  consola.success('Sent metrics to:', baseUrl);
}

(async () => {
  const browser = await puppeteer.launch({
    ignoreHTTPSErrors: true,
    headless: true,
  });

  const client = axios.create({
    httpsAgent: new https.Agent({
      rejectUnauthorized: false,
    }),
  });

  const baseUrl = 'https://localhost:8443';
  const metrics = calculateMeasurements(await gatherRawMetrics(browser, baseUrl));
  const currentTime = Math.floor(Date.now() / 1000);
  await browser.close();
  await postMetrics(client, baseUrl, metrics, currentTime);
})();
