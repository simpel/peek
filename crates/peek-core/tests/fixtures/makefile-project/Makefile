.PHONY: build test clean

build:
	cargo build --release

test: build
	cargo test

clean:
	rm -rf target

install: build
	cp target/release/app /usr/local/bin/

# This should NOT be picked up
%.o: %.c
	$(CC) -c $< -o $@
