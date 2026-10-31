.PHONY: test quick strict clean

test:
	./run.sh

quick:
	AI_TEST_SKIP_RUNTIME=1 ./run.sh

strict:
	AI_TEST_STRICT=1 ./run.sh

clean:
	rm -rf .work artifacts
