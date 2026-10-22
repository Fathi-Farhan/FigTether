/** The manifest selects the WebSocket endpoint for every plugin bundle. */
export function selectManifestPort(manifest, override) {
  const endpoints = manifest.networkAccess?.devAllowedDomains ?? [];
  const ports = endpoints
    .filter((domain) => typeof domain === "string")
    .map((domain) => /^ws:\/\/localhost:(\d+)$/.exec(domain)?.[1])
    .filter(Boolean)
    .map(Number);
  if (!ports.length || ports.some((port) => port < 1 || port > 65535 || port !== ports[0]))
    throw Error("Manifest must name one ws://localhost:<port> endpoint (1 to 65535)");
  const port = ports[0];
  if (override !== undefined && override !== String(port))
    throw Error(
      "FIGTETHER_PORT disagrees with the manifest. Generate the manifest for the new port first.",
    );
  return port;
}
