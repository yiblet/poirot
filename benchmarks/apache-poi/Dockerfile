FROM maven:3.9.11-eclipse-temurin-21 AS build
WORKDIR /build
COPY benchmarks/java/pom.xml ./pom.xml
RUN mvn -B org.apache.maven.plugins:maven-dependency-plugin:3.8.1:copy-dependencies -DoutputDirectory=/build/lib -DincludeScope=runtime
COPY benchmarks/java/artifacts.sha256 ./artifacts.sha256
RUN sha256sum -c artifacts.sha256
COPY benchmarks/java/Bench.java benchmarks/apache-poi/Poi.java ./
RUN mkdir classes /licenses && javac -cp 'lib/*' -d classes Bench.java Poi.java \
    && java -version 2> /licenses/java-version.txt \
    && sha256sum lib/*.jar > /licenses/artifacts.sha256 \
    && cp pom.xml /licenses/pom.xml \
    && for jar in lib/*.jar; do name=$(basename "$jar"); mkdir "/licenses/$name"; cd "/licenses/$name"; unzip -j "/build/$jar" 'META-INF/LICENSE*' 'META-INF/NOTICE*' 'META-INF/*LICENSE*' 'META-INF/*NOTICE*' || test "$?" -eq 11; cd /build; done
FROM eclipse-temurin:21.0.9_10-jre-jammy
WORKDIR /benchmark
COPY --from=build /build/classes ./classes
COPY --from=build /build/lib ./lib
COPY --from=build /licenses /licenses
COPY LICENSES/MIT.txt /licenses/adapter-MIT.txt
RUN java -version 2> /licenses/java-version.txt
USER 65534:65534
ENTRYPOINT ["java", "-Dlog4j2.loggerContextFactory=org.apache.logging.log4j.simple.SimpleLoggerContextFactory", "-cp", "classes:lib/*", "Bench", "Poi"]
