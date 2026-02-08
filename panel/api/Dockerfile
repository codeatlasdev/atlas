FROM oven/bun:1 AS build
WORKDIR /app
COPY package.json bun.lock* ./
RUN bun install --frozen-lockfile 2>/dev/null || bun install
COPY src/ src/
COPY drizzle/ drizzle/
COPY drizzle.config.ts tsconfig.json ./

FROM oven/bun:1-slim
WORKDIR /app
COPY --from=build /app .
ENV NODE_ENV=production
EXPOSE 3100
CMD ["bun", "run", "src/index.ts"]
