import config from "../news.config.json" with { type: "json" };

// Shared with the desktop RSS reader. NEWS_SITE_URL can override the deployment/build URL.
export const feedConfig = {
  ...config,
  siteUrl: process.env.NEWS_SITE_URL ?? config.siteUrl,
};
