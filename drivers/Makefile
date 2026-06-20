# SPDX-License-Identifier: GPL-2.0

KDIR ?= ../../linux-env/linux
PWD := $(shell pwd)

default: modules

modules:
	$(MAKE) -C $(KDIR) M=$(PWD) LLVM=1 modules

clean:
	$(MAKE) -C $(KDIR) M=$(PWD) clean

.PHONY: default modules clean
