import Image from 'next/image';

export function Logo({ size = 24 }: { size?: number }) {
  return (
    <Image
      src="/logo.svg"
      alt="Atlas"
      width={size}
      height={size}
      className="dark:invert-0"
    />
  );
}
