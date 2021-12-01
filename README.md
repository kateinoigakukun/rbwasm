# rbwasm

```console
$ rbwasm --mapdir /lib::@ruby_root/lib -o static/ruby.wasm
info: installing wasi-sdk 14.0 into ".rbwasm/downloads/wasi-sdk-14.0"
info: installing rb-wasm-support 0.4.0 into ".rbwasm/downloads/rb-wasm-support-0.4.0"
info: downloading CRuby source into ".rbwasm/build/ruby-1eab6a92fee0ac78"
info: running ./autogen.sh
info: running ./configure
info: running make install
info: generating vfs image
info: running linker
info: running asyncify

$ wasmtime static/ruby.wasm -- -e "require 'rbconfig'; puts RbConfig::CONFIG['platform']" -I/embd-root/lib/ruby/3.1.0 -I/embd-root/lib/ruby/3.1.0/wasm32-wasi
wasm32-wasi

```
