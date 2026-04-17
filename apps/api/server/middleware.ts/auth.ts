import { defineHandler, HTTPError } from "nitro";

export default defineHandler((event) => {
    if (event.url.pathname.startsWith('/api')) {
        const key = event.req.headers.get("Authorization")?.split("Bearer ")[1];
        if(!key || key !== process.env.AUTH_KEY) throw new HTTPError("Unauthorized", { status: 400 });
    }
});