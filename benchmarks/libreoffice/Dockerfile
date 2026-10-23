FROM maven:3.9.11-eclipse-temurin-21 AS dependencies
WORKDIR /build
COPY benchmarks/java/pom.xml ./pom.xml
RUN mvn -B org.apache.maven.plugins:maven-dependency-plugin:3.8.1:copy-dependencies -DoutputDirectory=/build/lib -DincludeScope=runtime -DincludeGroupIds=com.fasterxml.jackson.core

FROM eclipse-temurin:21.0.9_10-jdk-jammy AS build
ARG LO_ARCHIVE=LibreOffice_26.8.1_Linux_x86-64_deb.tar.gz
ARG LO_SHA256=30903df3b9f61360d9660cd707de48cd2831469114492a5008ed58a0ac77d044
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates libxinerama1 libcairo2 libcups2 libdbus-1-3 libfontconfig1 libfreetype6 libglib2.0-0 libnspr4 libnss3 libsm6 libx11-6 libxext6 libxrender1 libxslt1.1 libxt6 libxrandr2 && rm -rf /var/lib/apt/lists/*
WORKDIR /build
RUN curl --fail --location --retry 3 "https://download.documentfoundation.org/libreoffice/stable/26.8.1/deb/x86_64/${LO_ARCHIVE}" -o office.tar.gz \
    && echo "${LO_SHA256}  office.tar.gz" | sha256sum -c - \
    && mkdir archive /licenses && tar -xzf office.tar.gz -C archive \
    && dpkg -i archive/*/DEBS/*.deb \
    && printf '%s  %s\n' "$LO_SHA256" "$LO_ARCHIVE" > /licenses/libreoffice-archive.sha256 \
    && dpkg-query -W > /licenses/packages.txt \
    && cp -a /usr/share/doc /licenses/distribution-docs \
    && rm -rf office.tar.gz archive
RUN apt-get update && apt-get install -y --no-install-recommends libx11-xcb1 libxcb-render0 libxcb-shm0 libice6 libxcomposite1 libxfixes3 libxdamage1 libxkbcommon0 libxkbcommon-x11-0 && rm -rf /var/lib/apt/lists/*
COPY --from=dependencies /build/lib ./lib
COPY benchmarks/java/artifacts.sha256 benchmarks/java/pom.xml ./
RUN grep 'lib/jackson-' artifacts.sha256 | sha256sum -c -
COPY benchmarks/java/Bench.java benchmarks/libreoffice/Calc.java ./
RUN mkdir classes && javac -cp 'lib/*:/opt/libreoffice26.8/program/classes/*' -d classes Bench.java Calc.java \
    && sha256sum lib/*.jar /opt/libreoffice26.8/program/classes/*.jar > /licenses/artifacts.sha256 \
    && dpkg-query -W > /licenses/packages.txt \
    && cp pom.xml /licenses/pom.xml \
    && cp -a /usr/share/doc/. /licenses/distribution-docs/ \
    && java -version 2> /licenses/java-version.txt \
    && /opt/libreoffice26.8/program/soffice --version > /licenses/libreoffice-version.txt
# Keep the matching native runtime, UNO jars, installed licenses and package provenance.
# JDK image is retained to avoid copying a partial native dependency closure.
WORKDIR /benchmark
COPY LICENSES/MIT.txt /licenses/adapter-MIT.txt
ENV HOME=/tmp SAL_USE_VCLPLUGIN=svp
USER 65534:65534
ENTRYPOINT ["java", "-Djava.library.path=/opt/libreoffice26.8/program", "-cp", "/build/classes:/build/lib/*:/opt/libreoffice26.8/program/classes/*", "Bench", "Calc"]
