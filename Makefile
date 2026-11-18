.PHONY: installer FORCE
installer: build/wobblescope-installer.sh build/wobblescope-installer.sh.sha256

build/wobblescope-installer.sh: installer/kpi.sh installer/installer.sh FORCE
	mkdir -p build
	{ \
	  cat installer/kpi.sh; \
	  echo; \
	  cat installer/installer.sh; \
	  echo; \
	  echo 'wobblescope_main "$$@"'; \
	} > build/wobblescope-installer.sh
	chmod +x build/wobblescope-installer.sh

build/wobblescope-installer.sh.sha256: build/wobblescope-installer.sh
	cd build && shasum -a 256 wobblescope-installer.sh > wobblescope-installer.sh.sha256

FORCE:
