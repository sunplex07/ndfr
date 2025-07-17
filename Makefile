# ndfr-media-helper

CC = gcc
CFLAGS := $(shell pkg-config --cflags gio-2.0) -Wall -O2
LIBS   := $(shell pkg-config --libs gio-2.0) -lm

TARGET = ndfr-media-helper

SRC = ndfr-media-helper.c

.PHONY: all clean

all: $(TARGET)

$(TARGET): $(SRC)
	$(CC) $(CFLAGS) -o $@ $^ $(LIBS)

clean:
	rm -f $(TARGET)
