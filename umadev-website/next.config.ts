import type { NextConfig } from "next";

const isGithubPages = process.env.GITHUB_PAGES === "true";
const githubPagesRepo = process.env.GITHUB_PAGES_REPO ?? "umadev";

const nextConfig: NextConfig = {
  ...(isGithubPages
    ? {
        output: "export",
        basePath: `/${githubPagesRepo}`,
        assetPrefix: `/${githubPagesRepo}/`,
        images: {
          unoptimized: true,
        },
      }
    : {}),
  turbopack: {
    root: __dirname,
  },
};

export default nextConfig;
