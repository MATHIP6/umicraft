#!/bin/sh
# Builds build/e2-agent.jar with JDK 8 (the JDK-internal ASM needs -XDignore.symbol.file).
set -e
cd "$(dirname "$0")"
JDK="${JDK8:-/c/Program Files/Eclipse Adoptium/jdk-8.0.512.9-hotspot}"
mkdir -p build/classes
env -u JAVA_TOOL_OPTIONS -u _JAVA_OPTIONS "$JDK/bin/javac" -XDignore.symbol.file -source 8 -target 8 -d build/classes src/e2/*.java
printf 'Premain-Class: e2.Agent\n' > build/manifest.txt
env -u JAVA_TOOL_OPTIONS -u _JAVA_OPTIONS "$JDK/bin/jar" cfm build/e2-agent.jar build/manifest.txt -C build/classes .
echo built build/e2-agent.jar
