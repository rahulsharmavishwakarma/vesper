# syntax=docker/dockerfile:1.7@sha256:a57df69d0ea827fb7266491f2813635de6f17269be881f696fbfdf2d83dda33e
FROM node:24.21.0-bookworm-slim@sha256:2fe369e969550cde8e867afc3fe370b260140cab4a23d467074295b42163d553 AS build

WORKDIR /workspace
COPY package.json package-lock.json ./
COPY apps/desktop/package.json apps/desktop/package.json
RUN --mount=type=cache,target=/root/.npm npm ci --ignore-scripts

COPY apps/desktop apps/desktop
COPY models/presets.json models/presets.json
RUN npm run build --workspace @transcribe/desktop

FROM nginxinc/nginx-unprivileged:1.30.4-alpine@sha256:adf5042a17f4ecdd200c595fa9ffd1be37efb18f89a830bd1a00e4ab4d59d42c
ARG APP_VERSION=dev
LABEL org.opencontainers.image.title="transcribe browser preview" \
      org.opencontainers.image.description="Static UI preview; microphone capture and local inference require the native desktop app" \
      org.opencontainers.image.version="${APP_VERSION}" \
      org.opencontainers.image.source="https://github.com/rahulsharmavishwakarma/transcribe"

COPY .github/container/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /workspace/apps/desktop/dist /usr/share/nginx/html

EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=3s --start-period=3s --retries=3 \
  CMD wget -qO- http://127.0.0.1:8080/healthz | grep -qx ok || exit 1
