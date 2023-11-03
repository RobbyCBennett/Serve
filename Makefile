PROGRAM_NAME := serve


ifeq ($(OS),Windows_NT)
	PROGRAM_PATH := target/release/$(PROGRAM_NAME).exe
else
	PROGRAM_PATH := target/release/$(PROGRAM_NAME)
endif

ifeq ($(OS),Windows_NT)
	INSTALL := @echo 1. Add a folder in \"/Program Files\"; echo 2. Copy the program there; echo 3. Add the folder to the PATH environment variable
else
	INSTALL := sudo cp $(PROGRAM_PATH) /usr/bin
endif


--:
	cargo b -r

debug:
	cargo r

install: --
	$(INSTALL)

run:
	cargo r -r

help:
	@echo make
	@echo make debug
	@echo make install
	@echo make run
