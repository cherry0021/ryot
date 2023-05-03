import { gqlClient } from "@/lib/services/api";
import type { NextPageWithLayout } from "../_app";
import LoggedIn from "@/lib/layouts/LoggedIn";
import { useMutation, useQuery } from "@tanstack/react-query";
import { MEDIA_DETAILS } from "@trackona/graphql/backend/queries";
import { useRouter } from "next/router";
import { useState, type ReactElement } from "react";
import {
	Alert,
	Button,
	Container,
	Flex,
	Group,
	Loader,
	Select,
	Stack,
	Title,
} from "@mantine/core";
import {
	ProgressUpdateAction,
	type ProgressUpdateMutationVariables,
} from "@trackona/generated/graphql/backend/graphql";
import { PROGRESS_UPDATE } from "@trackona/graphql/backend/mutations";
import { Verb, getVerb } from "@/lib/utilities";
import { DatePickerInput } from "@mantine/dates";
import { DateTime } from "luxon";
import { IconAlertCircle } from "@tabler/icons-react";

const Page: NextPageWithLayout = () => {
	const router = useRouter();
	const metadataId = parseInt(router.query.item?.toString() || "0");
	const onlySeason = !!router.query.onlySeason;

	const [selectedSeason, setSelectedSeason] = useState<string | null>(
		router.query.selectedSeason?.toString() || null,
	);
	const [selectedEpisode, setSelectedEpisode] = useState<string | null>(
		router.query.selectedEpisode?.toString() || null,
	);
	const [selectedDate, setSelectedDate] = useState<Date | null>(null);

	const details = useQuery({
		queryKey: ["details", metadataId],
		queryFn: async () => {
			const { mediaDetails } = await gqlClient.request(MEDIA_DETAILS, {
				metadataId: metadataId,
			});
			return mediaDetails;
		},
		staleTime: Infinity,
	});
	const progressUpdate = useMutation({
		mutationFn: async (
			variables: ProgressUpdateMutationVariables & { onlySeason: boolean },
		) => {
			if (onlySeason) {
				for (const episode of details.data?.showSpecifics?.seasons.find(
					(s) => s.seasonNumber.toString() === selectedSeason,
				)!.episodes || []) {
					await gqlClient.request(PROGRESS_UPDATE, {
						input: { episodeNumber: episode.episodeNumber, ...variables.input },
					});
				}
				return;
			}
			const { progressUpdate } = await gqlClient.request(
				PROGRESS_UPDATE,
				variables,
			);
			return progressUpdate;
		},
		onSuccess: () => {
			router.push(`/media?item=${metadataId}`);
		},
	});

	const title = details.data?.title;
	const mutationInput = {
		metadataId,
		episodeNumber: Number(selectedEpisode),
		seasonNumber: Number(selectedSeason),
	};

	return details.data && title ? (
		<Container size={"xs"}>
			<Stack>
				<Flex gap={"sm"}>
					<Title>{title}</Title>
					{progressUpdate.isLoading ? <Loader /> : null}
				</Flex>
				{details.data.showSpecifics ? (
					<>
						{onlySeason ? (
							<Alert color='yellow' icon={<IconAlertCircle size="1rem" />}>
								This will mark all episodes for Season {selectedSeason} as seen
							</Alert>
						) : null}
						<Title order={6}>
							Select season{onlySeason ? "" : " and episode"}
						</Title>
						<Select
							label="Season"
							data={details.data.showSpecifics.seasons.map((s) => ({
								label: `${s.seasonNumber}. ${s.name.toString()}`,
								value: s.seasonNumber.toString(),
							}))}
							onChange={setSelectedSeason}
							defaultValue={selectedSeason}
						/>
						{!onlySeason && selectedSeason ? (
							<Select
								label="Episode"
								data={details.data.showSpecifics.seasons
									.find((s) => s.seasonNumber === Number(selectedSeason))!
									.episodes.map((e) => ({
										label: `${e.episodeNumber}. ${e.name.toString()}`,
										value: e.episodeNumber.toString(),
									}))}
								onChange={setSelectedEpisode}
								defaultValue={selectedEpisode}
							/>
						) : null}
					</>
				) : null}
				<Title order={6}>
					When did you {getVerb(Verb.Read, details.data.type)} it?
				</Title>
				<Button
					variant="outline"
					onClick={async () => {
						await progressUpdate.mutateAsync({
							input: { action: ProgressUpdateAction.Now, ...mutationInput },
							onlySeason,
						});
					}}
				>
					Now
				</Button>
				<Button
					variant="outline"
					onClick={async () => {
						await progressUpdate.mutateAsync({
							input: {
								action: ProgressUpdateAction.InThePast,
								...mutationInput,
							},
							onlySeason,
						});
					}}
				>
					I do not remember
				</Button>
				<Group grow>
					<DatePickerInput
						dropdownType="modal"
						maxDate={new Date()}
						onChange={setSelectedDate}
						clearable
					/>
					<Button
						variant="outline"
						disabled={selectedDate === null}
						onClick={async () => {
							if (selectedDate)
								await progressUpdate.mutateAsync({
									input: {
										action: ProgressUpdateAction.InThePast,
										date: DateTime.fromJSDate(selectedDate).toISODate(),
										...mutationInput,
									},
									onlySeason,
								});
						}}
					>
						Custom date
					</Button>
				</Group>
			</Stack>
		</Container>
	) : null;
};

Page.getLayout = (page: ReactElement) => {
	return <LoggedIn>{page}</LoggedIn>;
};

export default Page;