import { Container, getContainer } from "@cloudflare/containers";

export class ContainerflareBasic extends Container {
  defaultPort = 8787;
  sleepAfter = "5m";
}

export default {
  async fetch(request, env) {
    // Route all requests to a single container instance; change the key to fan out per user.
    const instance = getContainer(env.CONTAINERFLARE_BASIC, "singleton");
    return instance.fetch(request);
  },
};
