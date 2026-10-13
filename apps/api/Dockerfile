FROM oven/bun:1.3.6 AS bun
FROM node:24-bookworm-slim AS builder
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends python3 make g++ \
    && rm -rf /var/lib/apt/lists/*
COPY package.json bun.lock ./
COPY apps/api/package.json apps/api/package.json
COPY apps/timetables/package.json apps/timetables/package.json
COPY packages/timetable-db/package.json packages/timetable-db/package.json
RUN bun install --frozen-lockfile
COPY apps/api apps/api
COPY packages/timetable-db packages/timetable-db
RUN cd apps/api && bun run build
RUN rm -rf node_modules apps/api/node_modules apps/timetables/node_modules packages/timetable-db/node_modules \
    && bun install --production --frozen-lockfile
RUN node -e "const D=require('./apps/api/node_modules/better-sqlite3');const db=new D(':memory:');db.close()"

FROM node:24-bookworm-slim
WORKDIR /app
COPY --from=builder /app/node_modules ./node_modules
COPY --from=builder /app/apps/api/node_modules ./apps/api/node_modules
COPY --from=builder /app/packages/timetable-db ./packages/timetable-db
COPY --from=builder /app/apps/api/.output ./apps/api/.output
USER node
ENV NODE_ENV=production HOST=0.0.0.0 PORT=3000 \
    TIMETABLES_DB_PATH=/data/timetables.sqlite \
    TIMETABLES_WORKER_PATH=/app/apps/api/.output/server/timetable-worker.mjs
EXPOSE 3000
CMD ["node", "apps/api/.output/server/index.mjs"]
