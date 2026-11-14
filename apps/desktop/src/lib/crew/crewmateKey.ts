/** The identity of one crewmate across crew servers: a crewmate id is only unique per server, so the server's hostd routing key is part of it. */
export const crewmateKey = (hostKey: string, crewmateId: string) => `${hostKey}\u0000${crewmateId}`;
