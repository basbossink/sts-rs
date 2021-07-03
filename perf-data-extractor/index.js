const axios = require('axios');
const commandLineArgs = require('command-line-args');
const commandLineUsage = require('command-line-usage');
const consola = require('consola');
const https = require('https');
const puppeteer = require('puppeteer');

/**
 * This function validates if the given string can be parsed as URL.
 * @param {String} candidateUrl - The string to validate.
 * @return {Boolean} true if the string can successfully be parsed by the
 * URL class, false otherwise.
 */
function isValidUrl(candidateUrl) {
  try {
    new URL(candidateUrl);
    return true;
  } catch (_) {
    return false;
  }
}

/**
 * An object combining the raw metrics available from puppeteer and the browser.
 * @typedef {Object} RawMetrics
 * @property {Object} pageMetrics - The metrics provided by the puppeteer API.
 * @property {Object[]} performanceEntries - The performance entries available from
 * the browser
 */

/**
 * This function gathers the raw performance metrics from
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

const helpOptionAlias = 'h';
const helpOptionName = 'help';
const stsRsHostOptionAlias = 's';
const stsRsHostOptionName = 'sts-rs-host';
const targetOptionAlias = 't';
const targetOptionName = 'target';

const optionDefinitions = [
  {name: targetOptionName, alias: targetOptionAlias},
  {name: stsRsHostOptionName, alias: stsRsHostOptionAlias},
  {name: helpOptionName, alias: helpOptionAlias},
];

const options = commandLineArgs(optionDefinitions);

(() => {
  if (!options.hasOwnProperty(targetOptionName) ||
     !isValidUrl(options[targetOptionName]) ||
     !options.hasOwnProperty(stsRsHostOptionName) ||
     !isValidUrl(options[stsRsHostOptionName]) ||
     options[helpOptionName]) {
    const sections = [
      {
        header: 'This application gathers performance metrics and posts them to an sts-rs end-point.',
        content: 'This application is intended to be used together with an sts-rs {italic backend} it ' +
          'visits a URL and posts some calculated performance metrics to the sts-rs end-point provided.',
      },
      {
        header: 'Options',
        optionList: [
          {
            name: targetOptionName,
            alias: targetOptionAlias,
            typeLabel: '{underline URL}',
            description: 'The page to visit such that its performance characteristics can be measured.',
          },
          {
            name: stsRsHostOptionName,
            alias: stsRsHostOptionAlias,
            typeLabel: '{underline URL}',
            description: 'The URL for the sts-r backend server to which the performance results will be ' +
              'published.',
          },
          {
            name: helpOptionName,
            alias: helpOptionAlias,
            description: 'Print this usage guide.',
            type: Boolean,
          },
        ],
      },
    ];
    const usage = commandLineUsage(sections);
    console.log(usage);
  } else {
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

      const metrics = calculateMeasurements(await gatherRawMetrics(browser, options[targetOptionName]));
      const currentTime = Math.floor(Date.now() / 1000);
      await browser.close();
      await postMetrics(client, options[stsRsHostOptionName], metrics, currentTime);
    })();
  }
})();
