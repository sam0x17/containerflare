import { Container, getContainer } from "@cloudflare/containers";

const METADATA_HEADER = "x-containerflare-metadata";

export class ContainerflareBasic extends Container {
  defaultPort = 8787;
  sleepAfter = "5m";
}

export default {
  async fetch(request, env) {
    // Route all requests to a single container instance; change the key to fan out per user.
    const instance = getContainer(env.CONTAINERFLARE_BASIC, "singleton");
    const metadata = buildMetadata(request, env);
    const headers = new Headers(request.headers);
    headers.set(METADATA_HEADER, JSON.stringify(metadata));
    const proxiedRequest = new Request(request, { headers });
    return instance.fetch(proxiedRequest);
  },
};

function buildMetadata(request, env) {
  const url = new URL(request.url);
  const metadata = {
    request_id:
      request.headers.get("cf-ray") ??
      (globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`),
    colo: request.cf?.colo,
    region: request.cf?.region,
    country: request.cf?.country,
    client_ip: request.headers.get("cf-connecting-ip") ?? undefined,
    host: url.host,
    scheme: url.protocol.replace(":", ""),
    worker_name: env.CONTAINERFLARE_WORKER,
    method: request.method,
    path: url.pathname + url.search,
    raw_url: url.toString(),
  };

  return metadata;
}
