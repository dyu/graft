#!/bin/sh

CURRENT_DIR=$PWD
# locate
if [ -z "$BASH_SOURCE" ]; then
    SCRIPT_DIR=`dirname "$(readlink -f $0)"`
elif [ -e '/bin/zsh' ]; then
    F=`/bin/zsh -c "print -lr -- $BASH_SOURCE(:A)"`
    SCRIPT_DIR=`dirname $F`
elif [ -e '/usr/bin/realpath' ]; then
    F=`/usr/bin/realpath $BASH_SOURCE`
    SCRIPT_DIR=`dirname $F`
else
    F=$BASH_SOURCE
    while [ -h "$F" ]; do F="$(readlink $F)"; done
    SCRIPT_DIR=`dirname $F`
fi
# change pwd
cd $SCRIPT_DIR

REL_CLANG_LIB='target/release/libclang.dylib'

UNAME=`uname`
[ "$UNAME" = 'Darwin' ] && \
[ ! -e "$REL_CLANG_LIB" ] && \
mkdir -p target/release && \
ln -s /Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/libclang.dylib $REL_CLANG_LIB

cargo build --release
