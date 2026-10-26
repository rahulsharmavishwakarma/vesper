# syntax=docker/dockerfile:1.7@sha256:a57df69d0ea827fb7266491f2813635de6f17269be881f696fbfdf2d83dda33e
FROM node:26.11.1-bookworm-slim@sha256:86f07bc9c5dce4578cf37e5a418b7bfc7f817cda25cde66e2b66e95ed86c4567 AS build

WORKDIR /workspace
COPY package.json package-lock.json ./
COPY apps/desktop/package.json apps/desktop/package.json
RUN --mount=type=cache,target=/root/.npm npm ci --ignore-scripts

COPY apps/desktop apps/desktop
COPY models/presets.json models/presets.json
RUN npm run build --workspace @vesper/desktop

FROM nginxinc/nginx-unprivileged:1.31.7-alpine@sha256:b5047b9b76845eac124358373c6852377c52e4b05834a47647e955dd91cb8f1e
ARG APP_VERSION=dev
LABEL org.opencontainers.image.title="Vesper browser preview" \
      org.opencontainers.image.description="Static UI preview; microphone capture and local inference require the native desktop app" \
      org.opencontainers.image.version="${APP_VERSION}" \
      org.opencontainers.image.source="https://github.com/rahulsharmavishwakarma/transcribe"

COPY .github/container/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /workspace/apps/desktop/dist /usr/share/nginx/html

EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=3s --start-period=3s --retries=3 \
  CMD wget -qO- http://127.0.0.1:8080/healthz | grep -qx ok || exit 1
