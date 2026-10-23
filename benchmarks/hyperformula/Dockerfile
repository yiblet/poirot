FROM node:24.10.0-bookworm-slim
WORKDIR /benchmark
COPY benchmarks/hyperformula/package.json benchmarks/hyperformula/package-lock.json ./
RUN npm ci --omit=dev --ignore-scripts --no-audit --no-fund
COPY benchmarks/hyperformula/adapter.cjs ./adapter.cjs
COPY benchmarks/hyperformula/COPYING ./COPYING
USER 65534:65534
ENTRYPOINT ["node", "adapter.cjs"]
