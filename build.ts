const target = process.argv[2] // e.g. "bun-linux-x64"
const outfile = process.argv[3] || "./dist/atlas"

const args = ["bun", "build", "--compile", "--minify", "./src/index.ts", "--outfile", outfile]
if (target) args.push("--target", target)

const proc = Bun.spawn(args, { stdout: "inherit", stderr: "inherit" })
const exitCode = await proc.exited
if (exitCode !== 0) process.exit(exitCode)

console.log(`Compiled: ${outfile}`)
